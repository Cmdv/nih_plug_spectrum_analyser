# Architecture Refactoring Plan - Plugin Learn

## Overview
This document outlines the architectural improvements needed to align plugin-learn with professional audio plugin patterns demonstrated by Diopser. The current implementation works but has grown organically and needs structural improvements.

## Current Architecture Issues

### 1. **Inconsistent Data Sharing Patterns**
```rust
// CURRENT - Mixed patterns
spectrum_data: Arc<RwLock<Vec<f32>>>,           // Shared mutable data
peak_level_left: Arc<AtomicF32>,                // Atomic values
audio_engine: Option<Arc<Mutex<AudioEngine>>>,  // Optional wrapper
sample_buffer_engine: Arc<Mutex<SampleBufferEngine>>, // Nested ownership
```

### 2. **Conditional Initialization** 
```rust
// CURRENT - Deferred initialization
audio_engine: None,  // Created in initialize(), causes Option<> wrapper
```

### 3. **Scattered UI Data**
```rust
// CURRENT - Individual fields in EditorInitFlags
EditorInitFlags {
    audio_engine: Arc<Mutex<AudioEngine>>,
    params: Arc<PluginLearnParams>,
    spectrum_data: Arc<RwLock<Vec<f32>>>,
    peak_level_left: Arc<AtomicF32>,
    peak_level_right: Arc<AtomicF32>,
}
```

### 4. **Mixed Responsibilities**
- Audio engines contain UI-specific processing logic
- Spectrum analysis mixed with display code
- Parameter management scattered across files

## Target Architecture (Following Diopser Pattern)

### 1. **Main Plugin Structure**
```rust
pub struct PluginLearn {
    // CORE PLUGIN RESPONSIBILITIES
    params: Arc<PluginLearnParams>,
    
    // AUDIO PROCESSING - Direct ownership, no Option<>
    audio_engine: AudioEngine,                    // Remove Arc<Mutex<>> wrapper
    
    // SHARED STATE - Minimal, atomic where possible
    sample_rate: Arc<AtomicF32>,                  // Shared with UI
    
    // UI COMMUNICATION - Separated input/output
    spectrum_input: SpectrumInput,                // Audio thread only
    spectrum_output: Arc<Mutex<SpectrumOutput>>,  // UI thread only
    meter_input: MeterInput,                      // Audio thread only  
    meter_output: Arc<MeterOutput>,               // UI thread only
    
    // UI STATE
    iced_state: Arc<IcedState>,
}
```

## Refactoring Plan

## Phase 1: Core Data Structure Refactoring

### 1.1 Create Separated Communication Channels

**File: `src/audio/spectrum_communication.rs` (NEW)**
```rust
// Following Diopser's SpectrumInput/SpectrumOutput pattern
pub struct SpectrumInput {
    fft_engine: FFTEngine,                        // Owns FFT processing
    triple_buffer_input: triple_buffer::Input<SpectrumData>,
}

pub struct SpectrumOutput {
    triple_buffer_output: Arc<Mutex<triple_buffer::Output<SpectrumData>>>,
}

impl SpectrumInput {
    pub fn new() -> (SpectrumInput, SpectrumOutput) {
        let (input, output) = TripleBuffer::new(&Default::default()).split();
        (
            SpectrumInput { 
                fft_engine: FFTEngine::new(),
                triple_buffer_input: input 
            },
            SpectrumOutput { 
                triple_buffer_output: Arc::new(Mutex::new(output)) 
            }
        )
    }
    
    // Called from audio thread only
    pub fn process(&mut self, buffer: &Buffer) {
        let spectrum_data = self.fft_engine.compute_spectrum(buffer);
        self.triple_buffer_input.write(spectrum_data);
    }
}
```

**File: `src/audio/meter_communication.rs` (NEW)**
```rust
// Similar pattern for meter data
pub struct MeterInput {
    peak_left: f32,
    peak_right: f32,
    smoothing_engine: MeterSmoothingEngine,
}

pub struct MeterOutput {
    peak_levels: Arc<(AtomicF32, AtomicF32)>,  // (left, right)
}

impl MeterInput {
    pub fn new() -> (MeterInput, MeterOutput) {
        let peak_levels = Arc::new((
            AtomicF32::new(util::MINUS_INFINITY_DB),
            AtomicF32::new(util::MINUS_INFINITY_DB),
        ));
        
        (
            MeterInput {
                peak_left: 0.0,
                peak_right: 0.0,
                smoothing_engine: MeterSmoothingEngine::new(),
            },
            MeterOutput { peak_levels }
        )
    }
    
    pub fn process(&mut self, buffer: &Buffer) {
        // Process peaks and update atomic values
        let (left_db, right_db) = self.smoothing_engine.process_peaks(buffer);
        self.peak_levels.0.store(left_db, Ordering::Relaxed);
        self.peak_levels.1.store(right_db, Ordering::Relaxed);
    }
}
```

### 1.2 Refactor Main Plugin Structure

**File: `src/lib.rs` (MAJOR CHANGES)**
```rust
pub struct PluginLearn {
    // CORE - Direct ownership, no Option<>
    params: Arc<PluginLearnParams>,
    audio_engine: AudioEngine,                    // Remove wrapper, init in default()
    
    // SHARED STATE - Minimal
    sample_rate: Arc<AtomicF32>,
    
    // COMMUNICATION CHANNELS - Separated
    spectrum_input: SpectrumInput,
    spectrum_output: SpectrumOutput,
    meter_input: MeterInput,
    meter_output: MeterOutput,
    
    // UI STATE
    iced_state: Arc<IcedState>,
}

impl Default for PluginLearn {
    fn default() -> Self {
        let sample_rate = Arc::new(AtomicF32::new(44100.0));
        let (spectrum_input, spectrum_output) = SpectrumInput::new();
        let (meter_input, meter_output) = MeterInput::new();
        
        Self {
            params: Arc::new(PluginLearnParams::new(sample_rate.clone())),
            
            // INITIALIZE DIRECTLY - No Option<> wrapper
            audio_engine: AudioEngine::new(sample_rate.clone()),
            
            sample_rate,
            spectrum_input,
            spectrum_output,
            meter_input,
            meter_output,
            iced_state: IcedState::from_size(800, 600),
        }
    }
}

impl Plugin for PluginLearn {
    fn process(&mut self, buffer: &mut Buffer, ...) -> ProcessStatus {
        // CLEAN SEPARATION
        
        // 1. Pre-gain spectrum analysis
        self.spectrum_input.process(buffer);
        
        // 2. Apply audio effects
        self.audio_engine.process(buffer, &self.params);
        
        // 3. Post-gain meter analysis  
        self.meter_input.process(buffer);
        
        ProcessStatus::Normal
    }
}
```

### 1.3 Create Grouped UI Data Structure

**File: `src/editor.rs` (REFACTOR)**
```rust
// Following Diopser's Data pattern
#[derive(Clone)]
pub struct EditorData {
    // PARAMETER ACCESS
    pub params: Arc<PluginLearnParams>,
    
    // AUDIO STATE - Read-only from UI
    pub sample_rate: Arc<AtomicF32>,
    
    // DISPLAY DATA - Separated communication channels
    pub spectrum_output: SpectrumOutput,
    pub meter_output: MeterOutput,
}

pub struct EditorInitFlags {
    pub editor_data: EditorData,  // Single grouped struct
}

impl IcedEditor for PluginEditor {
    type InitializationFlags = EditorInitFlags;
    
    fn new(flags: Self::InitializationFlags, context: Arc<dyn GuiContext>) -> (Self, Task<Self::Message>) {
        let EditorData { params, sample_rate, spectrum_output, meter_output } = flags.editor_data;
        
        let editor = Self {
            params: params.clone(),
            
            // DISPLAY COMPONENTS - Pure rendering
            spectrum_display: SpectrumDisplay::new(spectrum_output, sample_rate.clone()),
            meter_display: MeterDisplay::new(meter_output),
            knob_display: GainKnobDisplay::new(params),
            
            context,
        };
        
        (editor, Task::none())
    }
}
```

## Phase 2: Engine Refactoring

### 2.1 Simplify Audio Engine

**File: `src/audio/audio_engine.rs` (REFACTOR)**
```rust
// Remove Arc<Mutex<>> wrapper, direct ownership
pub struct AudioEngine {
    sample_buffer_engine: SampleBufferEngine,  // Direct ownership
    sample_rate: Arc<AtomicF32>,                // Shared reference only
}

impl AudioEngine {
    pub fn new(sample_rate: Arc<AtomicF32>) -> Self {
        Self {
            sample_buffer_engine: SampleBufferEngine::new(),
            sample_rate,
        }
    }
    
    // Clean interface - no shared mutability needed
    pub fn process(&mut self, buffer: &mut Buffer, params: &PluginLearnParams) {
        // Apply gain with parameter smoothing
        for mut channel_samples in buffer.iter_samples() {
            let gain = params.gain.smoothed.next();
            for sample in channel_samples.iter_mut() {
                *sample *= gain;
            }
        }
        
        // Update sample buffer for any other processing
        self.sample_buffer_engine.process(buffer);
    }
}
```

### 2.2 Refactor Display Components

**File: `src/ui/spectrum_display.rs` (REFACTOR)**
```rust
// Pure display component - no processing logic
pub struct SpectrumDisplay {
    spectrum_output: SpectrumOutput,  // Communication channel
    sample_rate: Arc<AtomicF32>,      // For frequency calculation
}

impl SpectrumDisplay {
    pub fn new(spectrum_output: SpectrumOutput, sample_rate: Arc<AtomicF32>) -> Self {
        Self { spectrum_output, sample_rate }
    }
}

impl<Message> Program<Message, Theme> for SpectrumDisplay {
    fn draw(&self, ...) -> Vec<Geometry> {
        // READ ONLY - No processing
        let spectrum_data = self.spectrum_output.read();
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        
        // Pure rendering logic
        self.render_spectrum(frame, spectrum_data, sample_rate)
    }
}
```

**File: `src/ui/meter_display.rs` (REFACTOR)**
```rust
pub struct MeterDisplay {
    meter_output: MeterOutput,  // Communication channel
}

impl<Message> Program<Message, Theme> for MeterDisplay {
    fn draw(&self, ...) -> Vec<Geometry> {
        // READ ATOMIC VALUES
        let (left_db, right_db) = (
            self.meter_output.peak_levels.0.load(Ordering::Relaxed),
            self.meter_output.peak_levels.1.load(Ordering::Relaxed),
        );
        
        // Pure rendering
        self.render_meters(frame, left_db, right_db)
    }
}
```

## Phase 3: Parameter System Improvements

### 3.1 Centralized Parameter Construction

**File: `src/lib.rs` (PARAMETER REFACTOR)**
```rust
impl PluginLearnParams {
    pub fn new(sample_rate: Arc<AtomicF32>) -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(5.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

// Expose range functions for UI widgets (like Diopser)
pub fn gain_range() -> FloatRange {
    FloatRange::Skewed {
        min: util::db_to_gain(-30.0),
        max: util::db_to_gain(30.0),
        factor: FloatRange::gain_skew_factor(-30.0, 30.0),
    }
}
```

## Phase 4: File Structure Reorganization

### 4.1 New File Structure
```
src/
├── lib.rs                      # Main plugin (REFACTORED)
├── editor.rs                   # UI coordination (REFACTORED)  
├── audio/
│   ├── mod.rs                  # Module exports
│   ├── audio_engine.rs         # Core audio processing (SIMPLIFIED)
│   ├── spectrum_communication.rs # NEW - Spectrum input/output
│   ├── meter_communication.rs  # NEW - Meter input/output
│   ├── constants.rs            # Audio constants
│   └── engines/                # NEW - Processing engines
│       ├── mod.rs
│       ├── fft_engine.rs       # Pure FFT processing
│       ├── sample_buffer_engine.rs # Buffer management
│       └── meter_smoothing_engine.rs # Meter calculations
└── ui/
    ├── mod.rs                  # Module exports
    ├── spectrum_display.rs     # Pure spectrum rendering (REFACTORED)
    ├── meter_display.rs        # Pure meter rendering (REFACTORED)  
    ├── gain_knob_display.rs    # Knob widget
    └── style/
        ├── mod.rs
        └── theme.rs            # UI theme
```

### 4.2 Remove Obsolete Files
- `src/audio/spectrum_engine.rs` → Logic moved to `spectrum_communication.rs`
- `src/audio/meter_engine.rs` → Logic moved to `meter_communication.rs`
- `src/ui/svg_knob.rs` → Consolidate into `gain_knob_display.rs`

## Implementation Order

### **Step 1: Communication Channels**
1. Create `spectrum_communication.rs` with `SpectrumInput`/`SpectrumOutput`
2. Create `meter_communication.rs` with `MeterInput`/`MeterOutput`
3. Test basic data flow

### **Step 2: Main Plugin Refactor**
1. Remove `Option<>` wrapper from `audio_engine`
2. Update `Default::default()` to initialize everything
3. Simplify `process()` method with new communication channels
4. Test audio processing still works

### **Step 3: UI Data Grouping**
1. Create `EditorData` struct
2. Refactor `EditorInitFlags` to use grouped data
3. Update editor creation in `lib.rs`
4. Test UI still displays correctly

### **Step 4: Display Component Refactor**
1. Update `SpectrumDisplay` to use `SpectrumOutput`
2. Update `MeterDisplay` to use `MeterOutput` 
3. Remove processing logic from display components
4. Test spectrum and meters still work

### **Step 5: File Structure Cleanup**
1. Move processing engines to `engines/` subfolder
2. Remove obsolete files
3. Update module imports
4. Final testing

## Expected Benefits

### **1. Clearer Separation of Concerns**
- Audio thread: Only writes to communication channels
- UI thread: Only reads from communication channels
- No shared mutable state between threads

### **2. Better Performance**
- Lock-free communication for spectrum data
- Reduced mutex contention
- More predictable real-time behavior

### **3. Easier Maintenance**
- Clear data flow patterns
- Centralized parameter management
- Consistent file organization

### **4. Professional Architecture**
- Follows established plugin patterns
- Easier to extend with new features
- Better code organization

## Migration Strategy

### **Testing at Each Step**
1. **Compile tests**: Ensure code builds after each change
2. **Audio tests**: Verify audio processing still works
3. **UI tests**: Check that spectrum and meters display correctly
4. **Parameter tests**: Confirm gain control still functions

### **Rollback Plan**
- Keep current implementation in git branch
- Implement changes incrementally
- Test each phase before proceeding
- Can revert individual changes if needed

---

This refactoring will transform plugin-learn from a "works but messy" implementation into a professional, maintainable audio plugin architecture following industry best practices.