# Diopser Plugin - Communication Architecture Analysis

## Overview
Diopser is a phase rotation plugin that demonstrates clean separation between audio processing and UI, with sophisticated spectrum analysis and parameter management. This document analyzes the communication patterns for comparison with other plugin architectures.

## File Structure & Responsibilities

```
src/
├── lib.rs              # Main plugin + audio processing
├── params.rs           # Centralized parameter definitions
├── spectrum.rs         # Lock-free spectrum communication
├── filter.rs           # Audio processing (biquad filters)  
└── editor/
    ├── editor.rs       # Main UI layout & editor creation
    ├── analyzer.rs     # Spectrum analyzer widget
    ├── safe_mode.rs    # Safe mode parameter restricter
    ├── xy_pad.rs       # Custom XY pad widget
    ├── slider.rs       # Restricted parameter slider
    └── button.rs       # Custom button widget
```

## Core Communication Patterns

### 1. Main Plugin Structure (`lib.rs`)

```rust
pub struct Diopser {
    // DIRECT OWNERSHIP - No Option<> wrappers
    params: Arc<DiopserParams>,
    sample_rate: Arc<AtomicF32>,               // Shared with UI
    filters: [filter::Biquad<f32x2>; MAX_NUM_FILTERS],
    bypass_smoother: Arc<Smoother<f32>>,
    should_update_filters: Arc<AtomicBool>,    // Shared with params
    
    // SPECTRUM COMMUNICATION - Triple buffer pattern
    spectrum_input: SpectrumInput,             // Audio thread
    spectrum_output: Arc<Mutex<SpectrumOutput>>, // UI thread
}
```

**Key Patterns:**
- **No conditional initialization**: Everything created in `Default::default()`
- **Arc<AtomicT>** for simple shared values (sample_rate, should_update_filters)
- **Arc<Mutex<T>>** only for complex UI data (SpectrumOutput)
- **Separation of concerns**: `spectrum_input` vs `spectrum_output`

### 2. Parameter Communication (`params.rs`)

#### Parameter Definition Pattern
```rust
#[derive(Params)]
pub struct DiopserParams {
    // UI STATE - Shared with editor
    #[persist = "editor-state"]
    pub editor_state: Arc<ViziaState>,
    
    // FEATURE FLAGS - Shared atomic state  
    #[persist = "safe-mode"]
    pub safe_mode: Arc<AtomicBool>,
    
    // AUDIO PARAMETERS - With callbacks for immediate updates
    #[id = "stages"]  
    pub filter_stages: IntParam,  // Callback → should_update_filters.store(true)
    
    #[id = "cutoff"]
    pub filter_frequency: FloatParam,  // Smoothed, no callback needed
    
    // ... more parameters
}
```

#### Parameter Constructor Pattern  
```rust
impl DiopserParams {
    pub fn new(
        sample_rate: Arc<AtomicF32>,           // Shared reference
        should_update_filters: Arc<AtomicBool>, // Shared reference  
        bypass_smoother: Arc<Smoother<f32>>,   // Shared reference
    ) -> Self {
        Self {
            // Parameters with callbacks for immediate updates
            filter_stages: IntParam::new("Filter Stages", 0, filter_stages_range())
                .with_callback({
                    let should_update_filters = should_update_filters.clone();
                    Arc::new(move |_| should_update_filters.store(true, Ordering::Release))
                }),
                
            // Smoothed parameters reference shared state in callbacks
            bypass: BoolParam::new("Bypass", false)
                .with_callback(Arc::new(move |value| {
                    bypass_smoother.set_target(
                        sample_rate.load(Ordering::Relaxed),
                        if value { 1.0 } else { 0.0 },
                    );
                })),
        }
    }
}
```

**Key Patterns:**
- **Centralized parameter struct** with all parameters
- **Shared Arc references** passed to parameter callbacks
- **Range functions** exposed for UI widgets: `filter_frequency_range()`
- **Atomic flags** for immediate parameter change notifications

### 3. Spectrum Communication (`spectrum.rs`)

#### Lock-Free Audio → UI Communication
```rust
// SPECTRUM DATA FLOW:
//
// Audio Thread:          UI Thread:
//   spectrum_input   →   spectrum_output  
//      ↓ write              ↑ read
//   triple_buffer    →   triple_buffer
//      (lock-free)

pub struct SpectrumInput {
    // Audio processing state
    stft: util::StftHelper,
    plan: Arc<dyn RealToComplex<f32>>,
    complex_fft_buffer: Vec<Complex32>,
    spectrum_result_buffer: Spectrum,
    
    // Lock-free communication
    triple_buffer_input: triple_buffer::Input<Spectrum>,
}

pub type SpectrumOutput = triple_buffer::Output<Spectrum>;

impl SpectrumInput {
    pub fn new(num_channels: usize) -> (SpectrumInput, SpectrumOutput) {
        let (triple_buffer_input, triple_buffer_output) =
            TripleBuffer::new(&[0.0; SPECTRUM_WINDOW_SIZE / 2 + 1]).split();
        
        // Return both ends of communication channel
        (SpectrumInput { triple_buffer_input, /* ... */ }, 
         triple_buffer_output)
    }
    
    // Called from audio thread (process())
    pub fn compute(&mut self, buffer: &Buffer) {
        self.stft.process_analyze_only(buffer, SPECTRUM_WINDOW_OVERLAP, |_, samples| {
            // FFT processing...
            // Write to triple buffer (lock-free)
            self.triple_buffer_input.write(self.spectrum_result_buffer);
        });
    }
}
```

**Key Patterns:**
- **Factory function** returns both ends: `(SpectrumInput, SpectrumOutput)`
- **Triple buffer** for lock-free communication (not Arc<Mutex>)
- **Audio thread**: Only writes via `SpectrumInput::compute()`
- **UI thread**: Only reads via `SpectrumOutput::read()`
- **Clear separation**: No shared mutable state

### 4. Editor Data Structure (`editor.rs`)

#### Grouped UI Data Pattern
```rust
#[derive(Lens, Clone)]
pub(crate) struct Data {
    // PARAMETER ACCESS
    pub(crate) params: Arc<DiopserParams>,
    
    // AUDIO STATE - Read-only from UI
    pub(crate) sample_rate: Arc<AtomicF32>,
    pub(crate) spectrum: Arc<Mutex<SpectrumOutput>>,
    
    // UI-SPECIFIC STATE
    pub(crate) safe_mode_clamper: SafeModeClamper,
}

// Editor creation pattern
pub(crate) fn create(editor_data: Data, editor_state: Arc<ViziaState>) -> Option<Box<dyn Editor>> {
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        // Register shared data for all widgets to access
        editor_data.clone().build(cx);
        
        // Build UI hierarchy
        VStack::new(cx, |cx| {
            top_bar(cx);
            spectrum_analyzer(cx);  // Uses Data::spectrum, Data::sample_rate
            other_params(cx);       // Uses Data::params
        });
    })
}
```

#### Editor Creation in Main Plugin
```rust
impl Plugin for Diopser {
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(
            editor::Data {
                params: self.params.clone(),           // Parameter access
                sample_rate: self.sample_rate.clone(), // Audio state
                spectrum: self.spectrum_output.clone(), // UI data
                safe_mode_clamper: SafeModeClamper::new(self.params.clone()),
            },
            self.params.editor_state.clone(), // UI state
        )
    }
}
```

**Key Patterns:**
- **Single data struct** groups all UI dependencies
- **Arc cloning** shares references, not data
- **No audio processing** in UI thread
- **Lens derive** for reactive UI updates

### 5. Custom Widget Communication

#### Spectrum Analyzer Widget
```rust
pub struct SpectrumAnalyzer {
    // SHARED DATA REFERENCES
    spectrum: Arc<Mutex<SpectrumOutput>>,     // Data source
    sample_rate: Arc<AtomicF32>,              // For frequency calculation
    x_renormalize_display: Box<dyn Fn(f32) -> f32>, // Safe mode integration
    frequency_range: FloatRange,              // Matches parameter range
}

impl SpectrumAnalyzer {
    // LENS-BASED CONSTRUCTION
    pub fn new<LSpectrum, LRate>(
        cx: &mut Context,
        spectrum: LSpectrum,                   // Lens to shared data
        sample_rate: LRate,                    // Lens to shared data  
        x_renormalize_display: impl Fn(f32) -> f32 + Clone + 'static,
    ) -> Handle<Self>
    where
        LSpectrum: Lens<Target = Arc<Mutex<SpectrumOutput>>>,
        LRate: Lens<Target = Arc<AtomicF32>>,
    {
        Self {
            spectrum: spectrum.get(cx),        // Extract from lens
            sample_rate: sample_rate.get(cx),  // Extract from lens
            x_renormalize_display: Box::new(x_renormalize_display),
            frequency_range: params::filter_frequency_range(), // Matches param
        }
    }
}

impl View for SpectrumAnalyzer {
    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        // READ SHARED DATA
        let mut spectrum = self.spectrum.lock().unwrap();
        let spectrum = spectrum.read();                    // Triple buffer read
        let nyquist = self.sample_rate.load(Ordering::Relaxed);
        
        // RENDER SPECTRUM - No audio processing, just display
        for (bin_idx, magnitude) in spectrum.iter().enumerate() {
            let frequency = (bin_idx as f32 / spectrum.len() as f32) * nyquist;
            let t = (self.x_renormalize_display)(self.frequency_range.normalize(frequency));
            // ... draw spectrum line
        }
    }
}
```

#### XY Pad Multi-Parameter Widget  
```rust
pub struct XyPad {
    // PARAMETER COMMUNICATION
    x_param_base: ParamWidgetBase,  // Handles parameter updates
    y_param_base: ParamWidgetBase,  // Handles parameter updates
    
    // SAFE MODE INTEGRATION
    x_renormalize_display: Box<dyn Fn(f32) -> f32>, // Display transform
    x_renormalize_event: Box<dyn Fn(f32) -> f32>,   // Event transform
}

impl XyPad {
    fn set_normalized_values(&self, cx: &mut EventContext, (x_value, y_value): (f32, f32)) {
        // PARAMETER UPDATE PATTERN
        self.y_param_base.set_normalized_value(cx, y_value); // Update parameter 
        self.x_param_base.set_normalized_value(cx, x_value); // Update parameter
        // ParamWidgetBase handles the ParamEvent generation
    }
    
    fn begin_set_parameters(&self, cx: &mut EventContext) {
        // GESTURE MANAGEMENT
        self.y_param_base.begin_set_parameter(cx);
        self.x_param_base.begin_set_parameter(cx);
    }
    
    fn end_set_parameters(&self, cx: &mut EventContext) {
        self.x_param_base.end_set_parameter(cx);
        self.y_param_base.end_set_parameter(cx);
    }
}
```

**Key Patterns:**
- **Lens-based data access** for reactive updates
- **ParamWidgetBase** handles parameter communication protocol
- **Range normalization** functions for safe mode integration
- **Gesture management** with begin/set/end pattern

### 6. Safe Mode Pattern (`safe_mode.rs`)

#### Parameter Range Restriction
```rust
#[derive(Clone)]
pub struct SafeModeClamper {
    enabled: Arc<AtomicBool>,                 // Shared state
    params: Arc<DiopserParams>,               // Parameter access
    
    // Pre-computed normalized ranges for efficiency
    filter_stages_restricted_normalized_min: f32,
    filter_stages_restricted_normalized_max: f32,
    filter_frequency_restricted_normalized_min: f32,
    filter_frequency_restricted_normalized_max: f32,
}

impl SafeModeClamper {
    // BIDIRECTIONAL RANGE MAPPING
    
    // For display: Full range → Restricted range
    pub fn filter_frequency_renormalize_display(&self, t: f32) -> f32 {
        if self.status() {
            let renormalized = (t - self.filter_frequency_restricted_normalized_min)
                / (self.filter_frequency_restricted_normalized_max
                    - self.filter_frequency_restricted_normalized_min);
            renormalized.clamp(0.0, 1.0)
        } else {
            t
        }
    }
    
    // For events: Restricted range → Full range  
    pub fn filter_frequency_renormalize_event(&self, t: f32) -> f32 {
        if self.status() {
            t * (self.filter_frequency_restricted_normalized_max
                - self.filter_frequency_restricted_normalized_min)
                + self.filter_frequency_restricted_normalized_min
        } else {
            t
        }
    }
}
```

**Key Patterns:**
- **Shared atomic state** for mode switching
- **Bidirectional mapping** functions (display ↔ event)
- **Pre-computed ranges** for performance
- **Parameter clamping** when enabling restrictions

## Communication Flow Summary

### Audio Thread → UI Thread
```
1. Audio processing (lib.rs:process())
   ↓
2. spectrum_input.compute(buffer) 
   ↓
3. Triple buffer write (lock-free)
   ↓
4. UI reads spectrum_output.read()
   ↓  
5. SpectrumAnalyzer::draw() displays data
```

### Parameter Updates: UI → Audio
```
1. Widget interaction (e.g., XyPad mouse move)
   ↓
2. param_base.set_normalized_value()
   ↓
3. ParamEvent generated
   ↓
4. NIH-plug parameter system
   ↓
5. Parameter callbacks trigger (e.g., should_update_filters.store(true))
   ↓
6. Audio thread checks flags in process()
```

### Shared State Pattern
```
Plugin (lib.rs):
  - Owns: spectrum_input, audio processing state
  - Shares: params, sample_rate, spectrum_output
  
Editor (editor.rs):  
  - Receives: shared Arc references via Data struct
  - Creates: UI widgets with lens access to shared data
  - No audio processing, only display/interaction
```

## Key Differences from Your Current Architecture

### ✅ Diopser's Strengths
1. **Clean separation**: `SpectrumInput` (audio) vs `SpectrumOutput` (UI)
2. **No Option wrappers**: Direct ownership with initialization in `Default::default()`
3. **Grouped UI data**: Single `Data` struct for editor dependencies  
4. **Lock-free spectrum**: Triple buffer instead of `Arc<RwLock<Vec<f32>>>`
5. **Parameter callbacks**: Immediate notifications via atomic flags
6. **Range functions**: Centralized parameter ranges shared with UI

### 🔄 Areas for Your Consideration
1. **Spectrum communication**: Consider `SpectrumInput`/`SpectrumOutput` pattern vs shared `Vec<f32>`
2. **Audio engine**: Remove `Option<Arc<Mutex<AudioEngine>>>` wrapper
3. **UI data grouping**: Create `EditorData` struct like Diopser's `Data`
4. **Parameter ranges**: Expose range functions for UI widgets
5. **Lock-free patterns**: Evaluate triple buffer vs `Arc<RwLock>` for spectrum data

### 📝 Note on UI Framework Differences
- **Diopser**: Uses Vizia with lens-based reactive patterns
- **Your Plugin**: Uses iced with message-based patterns
- **Communication patterns**: Remain similar despite different UI frameworks
- **Data sharing**: Arc references work the same in both frameworks

---

This analysis shows Diopser's mature approach to audio plugin architecture with clean separation of concerns and efficient communication patterns.

Yes! dasp has several useful modules that could enhance your plugin:

  Useful dasp Features for Your Plugin:

  1. Signal Processing & Envelopes

  - Peak envelope tracking - More sophisticated than your current peak detection
  - RMS calculation - Add RMS metering alongside your peak meters (standard in pro
  analyzers)
  - Envelope followers - Could improve your meter attack/release behavior

  2. Windowing Functions

  While you're using apodize for Blackman windows, dasp provides:
  - Built-in Hanning windows (lighter CPU than Blackman, still good for spectrum)
  - Integrates directly with dasp's signal chain

  3. Signal Manipulation

  - Interpolation - Useful if you want to add oversampling for more accurate peak
  detection
  - Signal mixing/routing - Clean abstractions for channel summing (you're doing
  this manually)

  4. Sample Type Conversions

  - Unified Sample trait for working with different bit depths
  - Could be useful if you expand to different sample formats

  Where dasp Would Help Most:

  RMS Metering - Add alongside your peak meters:
  // Your current code only does peak detection
  // dasp provides RMS calculation which gives perceived loudness
  use dasp::signal::{Signal, rms};

  Better Envelope Following - For smoother meter ballistics:
  // dasp's envelope module provides more sophisticated smoothing
  // than your current attack/release implementation

  Signal Chain Abstraction - Cleaner code for audio processing:
  // Instead of manual channel iteration, use dasp's signal iterators
  // Makes code more readable and potentially more efficient

  Trade-offs:

  - You're already using realfft (excellent for FFT) and apodize (great windowing
  library)
  - dasp would add another dependency but consolidate some DSP operations
  - Best for: RMS metering, envelope following, signal routing abstractions

  Would you like me to show you how to integrate RMS metering using dasp? That's
  probably the most valuable addition for a professional meter display.
