# Plugin Processing State Detection

## Problem Statement

When a plugin is turned off/bypassed in a DAW (like Bitwig), the spectrum analyzer freezes at the last frame instead of naturally decaying to silence. When the plugin is turned back on, it then shows an unwanted decay from the old frozen values.

**Expected behavior:** Spectrum should decay when plugin is turned off, not when turned back on.

## Root Cause Analysis

### DAW Plugin Lifecycle
- **Plugin ON**: DAW calls `process()` repeatedly with audio buffers
- **Plugin OFF**: DAW simply stops calling `process()` - no "final" callback
- **Plugin back ON**: DAW calls `reset()` then resumes `process()` calls

### Framework Differences

#### CLAP API (Lower Level)
- Has explicit `start_processing()` and `stop_processing()` callbacks
- Plugins know exactly when processing starts/stops
- Perfect for detecting bypass state

#### VST3 API (Lower Level)
- Has `setActive()` but only for major state changes (plugin removal)
- No explicit bypass callback for temporary on/off

#### nih_plug (Abstraction Layer)
- Simplifies both CLAP and VST3 into a unified Plugin trait
- Internally receives CLAP's `start_processing`/`stop_processing` but doesn't expose them
- **Already exposes start processing via `reset()`**
- **Missing: stop processing callback**

## Current Implementation Issues

Our spectrum display reads from a triple buffer that retains the last audio frame when `process()` stops. The UI has no way to know the plugin was turned off vs. audio just being steady.

## Research Findings

### nih_plug Internal Implementation
File: `src/wrapper/clap/wrapper.rs` (lines ~1100-1120)

```rust
unsafe extern "C" fn start_processing(plugin: *const clap_plugin) -> bool {
    let wrapper = &*((*plugin).plugin_data as *const Self);
    wrapper.is_processing.store(true, Ordering::SeqCst);

    // This calls our Plugin::reset() method!
    process_wrapper(|| wrapper.plugin.lock().reset());
    true
}

unsafe extern "C" fn stop_processing(plugin: *const clap_plugin) {
    let wrapper = &*((*plugin).plugin_data as *const Self);
    wrapper.is_processing.store(false, Ordering::SeqCst);

    // No plugin callback here - this is what we need to add!
}
```

**Key insight:** `start_processing` already calls our `reset()` method, but `stop_processing` doesn't call anything.

### Current Asymmetry
- **Processing starts**: `reset()` is called ✅
- **Processing stops**: Nothing is called ❌

This explains why we see "RESET called!" when turning the plugin back on, but nothing when turning it off.

## Solution Options

### 1. Time-Based Detection (Practical Workaround)

Track when `process()` was last called and trigger silence writing after a timeout.

```rust
// In lib.rs
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};
use std::time::{SystemTime, UNIX_EPOCH};

struct SAPlugin {
    // ... existing fields
    last_process_time: Arc<AtomicU64>,
}

impl Default for SAPlugin {
    fn default() -> Self {
        Self {
            // ... existing initialization
            last_process_time: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Plugin for SAPlugin {
    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        // Update timestamp every process call
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_process_time.store(current_time, Ordering::Relaxed);

        // ... existing processing
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        self.audio_spectrum_producer.process(buffer, sample_rate);
        self.audio_meter_producer.update_peaks(buffer);

        ProcessStatus::Normal
    }
}
```

Pass timestamp to UI and check for timeout:

```rust
// In spectrum_display.rs - add timestamp field back
pub struct SpectrumDisplay {
    spectrum_output: SpectrumConsumer,
    sample_rate: Arc<AtomicF32>,
    decay_state: Arc<Mutex<DecayState>>,
    last_process_time: Arc<AtomicU64>, // Add this back
}

fn get_display_spectrum(&self) -> SpectrumData {
    let current_spectrum = self.spectrum_output.read_or_silence();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let last_process = self.last_process_time.load(Ordering::Relaxed);
    let process_stopped = now.saturating_sub(last_process) > 300; // 300ms threshold

    if process_stopped {
        // Write silence to buffer and apply decay
        // This ensures clean transition when plugin turns back on
        return self.apply_decay_from_silence();
    } else {
        // Normal operation - existing logic
        self.handle_normal_spectrum_update(current_spectrum)
    }
}
```

### 2. Accept Current Behavior (Simplest)

Let the spectrum freeze when off and decay when turned back on. It's not ideal but functional.

### 3. UI-Only Solution (Current Implementation)

Our simplified approach detects when values are decreasing and applies natural decay:

```rust
fn get_display_spectrum(&self) -> SpectrumData {
    let current_spectrum = self.spectrum_output.read_or_silence();

    if let Some(last) = state.last_spectrum {
        let is_increasing = current_spectrum.iter()
            .zip(last.iter())
            .any(|(current, last)| current > last);

        if is_increasing {
            // New audio - update immediately
            current_spectrum
        } else {
            // Apply smooth decay
            let decayed = self.apply_decay(&last, time_delta);
            decayed.iter().zip(current_spectrum.iter())
                .map(|(&d, &c)| d.max(c))
                .collect()
        }
    } else {
        current_spectrum
    }
}
```

## Ideal Solution: Extend nih_plug

Add stop processing callback to the Plugin trait. Start processing is already handled by `reset()`.

### Proposed nih_plug Modification

**File: `src/plugin.rs`**
```rust
pub trait Plugin: Default + Send + Sync + 'static {
    // ... existing methods including reset()

    /// Called when audio processing stops (CLAP stop_processing, VST3 setActive(false))
    /// Counterpart to reset() which is called when processing starts
    fn stop_processing(&mut self) {
        // Default implementation does nothing
    }
}
```

**File: `src/wrapper/clap/wrapper.rs`**
```rust
unsafe extern "C" fn stop_processing(plugin: *const clap_plugin) {
    let wrapper = &*((*plugin).plugin_data as *const Self);
    wrapper.is_processing.store(false, Ordering::SeqCst);

    // Call plugin's stop_processing method - NEW
    process_wrapper(|| {
        wrapper.plugin.lock().stop_processing();
    });
}
```

**File: `src/wrapper/vst3/wrapper.rs`** (similar change needed for VST3)

### Usage in Our Plugin
```rust
impl Plugin for SAPlugin {
    fn stop_processing(&mut self) {
        // Write silence when processing stops
        self.audio_spectrum_producer.write_silence();
        nih_plug::nih_log!("Processing stopped - wrote silence");
    }

    fn reset(&mut self) {
        // Called when processing starts (existing behavior)
        nih_plug::nih_log!("Processing started");
    }
}
```

## Summary

We discovered that:
1. **CLAP has the callbacks we need** (`start_processing`/`stop_processing`)
2. **nih_plug already exposes start via `reset()`**
3. **nih_plug just needs to expose stop processing** - simple one-method addition
4. **The asymmetry causes our problem** - we get notified when processing starts but not when it stops

## Recommendation

1. **Short term**: Implement time-based detection (300ms timeout)
2. **Long term**: Submit feature request to nih_plug for `stop_processing()` callback
3. **Alternative**: Accept current behavior as "good enough" for this use case

The nih_plug modification would be minimal since start processing is already handled - just need to add the stop counterpart.