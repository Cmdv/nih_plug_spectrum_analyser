# Plugin Learn - Session Progress & Architecture Learnings

## 🎯 Current Status (Session 8 Completion)

### ✅ What's Working
1. **Spectrum Analyzer** - Beautiful Pro-Q style visualization with:
   - Smooth Bézier curves
   - A-weighting for perceptual accuracy
   - Logarithmic frequency scaling
   - Semi-transparent fill gradient
   - Grid lines and tick marks for frequency/dB reference
   - Real-time FFT processing with Hanning window

2. **Level Meters** - Vertical bars with:
   - Post-gain level tracking
   - Pro-Q style color gradient (green → yellow → red)
   - Attack/release smoothing
   - Left/right channel separation
   - Real-time updates via `AtomicF32`

3. **Gain Control** - Currently using `ParamSlider`:
   - Working parameter binding
   - Updates audio processing in real-time
   - **Issue**: Displays as horizontal slider, not circular knob

4. **Dark Theme** - Professional Pro-Q inspired theme:
   - Dark background throughout
   - Centralized `AudioTheme` with all colors/constants
   - Consistent styling across components

5. **Data Sharing Architecture** - Proper thread-safe communication:
   - `AtomicF32` for real-time level meters
   - `Arc<RwLock<Vec<f32>>>` for spectrum data
   - `Arc<Mutex<AudioProcessor>>` for audio processing
   - Triple buffer for waveform data (lock-free)

## 🏗️ Architecture Learnings

### NIH-plug + iced Integration

#### 1. **Parameter System Architecture**
```rust
// NIH-plug uses a message-based system for parameter updates
pub enum Message {
    ParamUpdate(nih_plug_iced::widgets::ParamMessage),
    Tick, // For UI refresh
}

// Parameter updates flow like this:
// User interaction → ParamMessage → IcedEditor::handle_param_message() → GuiContext → Audio thread
```

#### 2. **Editor Lifecycle**
- Editor can be created **before** `initialize()` is called
- Must handle case where audio processor doesn't exist yet
- Use `EditorInitFlags` to pass shared data from plugin to editor
- `create_iced_editor()` handles the `Editor` trait wrapper automatically

#### 3. **Thread-Safe Data Sharing Patterns**

```rust
// For real-time meter data (audio → UI, frequent updates)
use atomic_float::AtomicF32;  // NOT std::sync::atomic::AtomicF32
let peak_level = Arc<AtomicF32>::new(AtomicF32::new(-100.0));

// For spectrum data (audio → UI, buffer updates)
let spectrum_data = Arc<RwLock<Vec<f32>>>::new(vec![0.0; 1025]);

// For parameters (bidirectional)
let params = Arc<PluginLearnParams>;  // NIH-plug handles thread safety

// For audio processing state
let audio_processor = Arc<Mutex<AudioProcessor>>;
```

#### 4. **UI Update Pattern**
```rust
// Use subscription for regular redraws
fn subscription(&self, window_subs: &mut WindowSubs<Message>) -> Subscription<Message> {
    // This creates 60 FPS refresh
    window_subs.on_frame = Some(Arc::new(|| Some(Message::Tick)));
    Subscription::none()
}
```

### iced Widget System

#### 1. **Canvas vs Widgets**
- **Canvas**: Best for custom drawing (spectrum analyzer, custom meters)
- **SVG**: Good for scalable graphics (knobs, icons) - available via `svg` feature
- **ParamSlider**: Built-in NIH-plug widget, functional but limited styling

#### 2. **Layout System**
```rust
// iced uses builders for layout
row![
    container(left_widget).width(Length::FillPortion(4)),  // 80%
    container(right_widget).width(Length::FillPortion(1))  // 20%
]

// Styling with container::dark for built-in dark theme
container(content).style(container::dark)
```

#### 3. **Custom Canvas Widgets**
```rust
impl<Message> Program<Message, Theme> for CustomWidget {
    type State = ();
    
    fn draw(&self, _state: &Self::State, renderer: &Renderer, 
            _theme: &Theme, bounds: Rectangle, cursor: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // Drawing code here
        vec![frame.into_geometry()]
    }
}
```

## 🔧 Next Steps - Custom Knob Implementation

### Problem: ParamSlider displays as horizontal bar, not circular knob

### Solution Options:

1. **Custom Canvas Knob with Mouse Handling**
   - Implement full `Widget` trait (not just `Program`)
   - Handle mouse events for drag interaction
   - Calculate angle from mouse position
   - Update parameter via `ParamMessage`

2. **SVG-based Knob**
   - Generate dynamic SVG for knob appearance
   - Overlay invisible interaction area
   - Simpler than full widget implementation

3. **Styled ParamSlider**
   - Check if ParamSlider has style/appearance options
   - Potentially extend ParamSlider with custom rendering

### Recommended Approach: Custom Widget Implementation

```rust
// Skeleton for custom knob widget
pub struct Knob<'a> {
    param: &'a FloatParam,
    size: f32,
}

impl<'a> Widget<ParamMessage, Theme, Renderer> for Knob<'a> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }
    
    fn on_event(&mut self, state: &mut State, event: Event, ...) -> event::Status {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                // Start drag
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                // Calculate angle and update parameter
            }
            // etc...
        }
    }
    
    fn draw(&self, state: &State, renderer: &mut Renderer, ...) {
        // Draw circular knob with pointer
    }
}
```

## 📊 Performance Considerations

1. **Audio Thread Rules**
   - Never allocate memory
   - Never lock mutexes (use lock-free structures)
   - Use `AtomicF32` for simple values
   - Pre-allocate all buffers

2. **UI Thread Optimization**
   - Canvas redraws at 60 FPS via subscription
   - Use smoothing to reduce visual noise
   - Cache computed values when possible
   - Minimize allocations in draw methods

## 🐛 Known Issues & Fixes

1. **White Background** → Fixed with `container::dark` styling
2. **Knob Not Interactive** → Fixed with `ParamSlider` (but needs visual improvement)
3. **Level Meter No Data** → Fixed with `AtomicF32` sharing
4. **Compilation Errors** → Fixed imports and type issues

## 📝 Key Files & Their Roles

```
src/
├── lib.rs              # Plugin core - audio processing & parameter management
├── editor.rs           # UI layout & message handling
├── audio/
│   ├── buffer.rs       # Triple buffer for lock-free audio data
│   ├── fft.rs          # FFT processing for spectrum analysis
│   └── processor.rs    # Audio processing pipeline
└── ui/
    ├── spectrum.rs     # Canvas-based spectrum analyzer
    ├── meter.rs        # Canvas-based level meters
    └── style/
        ├── theme.rs    # Centralized colors & constants (AudioTheme)
        └── knob.rs     # Knob widget (needs conversion from canvas to widget)
```

## 🎨 Theme System

All UI constants centralized in `AudioTheme`:
- Colors (backgrounds, spectrum, meters, text)
- Dimensions (sizes, margins, padding)
- Audio parameters (dB ranges, frequencies)
- Animation timing (attack/release)
- Helper functions (dB conversion, color gradients)

## 💡 Important Lessons Learned

1. **AtomicF32 Location**: Use `atomic_float::AtomicF32`, not `std::sync::atomic::AtomicF32`
2. **Parameter Updates**: Must go through `IcedEditor::handle_param_message()`, not directly on context
3. **Editor Creation**: Can happen before audio initialization - handle gracefully
4. **Canvas Limitations**: Canvas `Program` trait is for drawing only - no interaction
5. **Theme Application**: Must apply `container::dark` to get dark backgrounds

## 🚀 Session 9 Goals - Pro-Q 3 Style Improvements

**Reference**: Comparing plugin-learn (left) with FabFilter Pro-Q 3 (right) to match professional appearance

### Priority Order:

1. **🎯 PRIORITY: Dead Space Elimination (#3)**
   - Remove remaining gap on right side of plugin
   - Ensure spectrum fills allocated space completely
   - Match Pro-Q's edge-to-edge spectrum coverage

2. **📊 LED-Style Level Meters (#1)**
   - Replace current meter with discrete LED segments
   - Mimic Pro-Q's small rectangular LED indicators
   - Remove meter labels, add current dB value display above meter

3. **🎛️ Circular Gain Knob (#6)**
   - Fix SVG knob to respond to circular mouse motion
   - Replace square ParamSlider with proper rotary control
   - Implement drag-to-rotate interaction from existing `svg_knob.rs`

4. **📝 Spectrum Text Labels (#5)**
   - Replace frequency tick marks with text values (20, 50, 100, 1k, etc.)
   - Use Pro-Q's yellow color scheme for frequency labels
   - Position text on canvas like y-axis labels

5. **🎨 Consistent Background (#7)**
   - Make all UI elements use spectrum's dark background color
   - Remove any lighter/inconsistent background areas
   - Match Pro-Q's uniform dark theme

6. **📐 Grid Improvements (#4)**
   - Move x-axis ticks from outside edge to canvas overlay
   - Match y-axis tick positioning style
   - Improve grid line consistency

### Implementation Notes:

- **svg_knob.rs exists** but needs mouse interaction implementation
- **Text rendering** may require iced's text features or canvas text drawing
- **LED meters** can use canvas with discrete rectangles
- **Dead space** likely caused by layout proportions or container margins

## 📚 Code Patterns from nih_plug_iced

### Parameter Message Handling Pattern
```rust
// From nih_plug_iced - this is how parameter updates work
// The IcedEditor trait provides this method:
fn handle_param_message(&self, message: ParamMessage) {
    let context = self.context();
    match message {
        ParamMessage::BeginSetParameter(p) => unsafe { 
            context.raw_begin_set_parameter(p) 
        },
        ParamMessage::SetParameterNormalized(p, v) => unsafe {
            context.raw_set_parameter_normalized(p, v)
        },
        ParamMessage::EndSetParameter(p) => unsafe { 
            context.raw_end_set_parameter(p) 
        },
    }
}
```

### ParamSlider Widget Pattern
```rust
// Key aspects of how ParamSlider handles interaction:
// 1. Stores parameter reference
pub struct ParamSlider<'a, P: Param> {
    param: &'a P,
    // sizing, styling fields...
}

// 2. Maps to ParamMessage in view
ParamSlider::new(&self.params.gain)
    .map(Message::ParamUpdate)

// 3. Internal state tracking for drag
struct State {
    drag_active: bool,
    granular_drag_start_x_value: Option<(f32, f32)>,
    // For fine control with Shift
}
```

### Widget Event Handling Pattern
```rust
// Pattern for handling mouse events in custom widgets:
fn on_event(&mut self, event: Event, bounds: Rectangle, cursor: Cursor) -> Status {
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
            if cursor.is_over(bounds) {
                // Start interaction
                self.state.dragging = true;
                self.state.drag_start = cursor.position();
                return Status::Captured;
            }
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            if self.state.dragging {
                // Update value based on movement
                let delta = calculate_delta(cursor.position(), self.state.drag_start);
                self.update_parameter(delta);
                return Status::Captured;
            }
        }
        Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
            if self.state.dragging {
                // End interaction
                self.state.dragging = false;
                return Status::Captured;
            }
        }
        _ => {}
    }
    Status::Ignored
}
```

### Window Subscription Pattern
```rust
// How to set up regular UI updates:
fn subscription(&self, window_subs: &mut WindowSubs<Message>) -> Subscription<Message> {
    // For 60 FPS updates
    window_subs.on_frame = Some(Arc::new(|| Some(Message::Tick)));
    
    // Can also subscribe to other events
    Subscription::none()
}
```

### Granular Control Pattern
```rust
// Pattern for fine control with Shift key:
const GRANULAR_DRAG_MULTIPLIER: f32 = 0.1;

if keyboard_modifiers.shift() {
    // Store start position for granular mode
    if granular_drag_start.is_none() {
        granular_drag_start = Some((mouse_x, current_value));
    }
    // Apply smaller change
    delta *= GRANULAR_DRAG_MULTIPLIER;
} else {
    // Reset granular mode
    granular_drag_start = None;
}
```

### Custom Widget Drawing Pattern
```rust
// How widgets typically structure their draw methods:
fn draw(&self, renderer: &mut Renderer, bounds: Rectangle, cursor: Cursor) {
    // 1. Calculate layout
    let center = bounds.center();
    let radius = bounds.width.min(bounds.height) / 2.0;
    
    // 2. Draw background elements
    renderer.fill_quad(
        Quad {
            bounds,
            border_radius: radius.into(),
            ..Default::default()
        },
        Background::Color(self.style.background_color),
    );
    
    // 3. Draw interactive elements with state-based styling
    let color = if cursor.is_over(bounds) {
        self.style.hover_color
    } else {
        self.style.normal_color
    };
    
    // 4. Draw value indicators
    let angle = value_to_angle(self.normalized_value());
    draw_pointer(renderer, center, radius, angle, color);
}
```

### Parameter Value Conversion Pattern
```rust
// How parameters handle value conversion:
// Linear gain stored, but displayed as dB
let gain_linear = self.param.value();
let gain_db = util::gain_to_db(gain_linear);
let normalized = self.param.normalized_value();

// Setting values
let new_linear = util::db_to_gain(new_db);
context.set_parameter(&self.param, new_linear);
```

### IcedEditor Initialization Pattern
```rust
// The complete pattern for editor setup:
impl IcedEditor for MyEditor {
    type Executor = Default;
    type Message = Message;
    type InitializationFlags = MyInitFlags;
    
    fn new(
        flags: Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Task<Self::Message>) {
        let editor = Self {
            params: flags.params,
            context,
            // other fields...
        };
        (editor, Task::none())
    }
    
    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::ParamUpdate(msg) => {
                self.handle_param_message(msg);
            }
            // other messages...
        }
        Task::none()
    }
    
    fn view(&self) -> Element<'_, Self::Message> {
        // Build UI here
    }
}
```

## 📚 Resources for Next Session

- [iced Custom Widget Guide](https://github.com/iced-rs/iced/tree/master/examples/custom_widget)
- [NIH-plug ParamSlider Source](https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_iced/src/widgets/param_slider.rs)
- [Circular Interaction Math](https://www.youtube.com/watch?v=v3sXpPsNQjI) (angle from mouse position)
- [Pro-Q Interface Analysis](https://www.fabfilter.com/products/pro-q-3-equalizer-plug-in)

## 🎯 End Goal

A fully functional gain plugin with Pro-Q quality visualization:
- Smooth, responsive spectrum analyzer
- Professional level meters with gradient colors
- Intuitive circular gain knob
- Cohesive dark theme
- Proper parameter management
- Thread-safe architecture

---

## 🔧 Architecture Refactoring Plan - Engine/Display Separation

### Current Naming Issues:
- Mixed audio processing and UI rendering logic
- Confusing component names (`audio_processor`, `spectrum_view`, etc.)
- Spectrum analysis logic embedded in UI code
- Duplicated processing patterns

### Proposed Engine/Display Pattern:

**Audio Processing Engines (Pure Data Logic):**
```rust
// Core audio effects processing
audio_engine: AudioEngine,

// Level analysis (smoothing, peak hold, RMS)
meter_engine: MeterEngine,  // ✅ COMPLETED

// FFT analysis and frequency processing  
spectrum_engine: SpectrumEngine,  // 🎯 NEXT: Extract from UI
```

**UI Display Components (Pure Presentation):**
```rust
// Renders spectrum curve, grid, labels
spectrum_display: SpectrumDisplay,

// Renders LED meter segments
meter_display: MeterDisplay,

// Renders circular knob widget
knob_display: KnobDisplay,
```

### Implementation Priority:

1. **🎯 NEXT: Spectrum Engine Extraction**
   - Create `audio/spectrum_engine.rs` following successful MeterProcessor pattern
   - Move FFT processing and frequency analysis from `ui/spectrum.rs`
   - Extract `SpectrumEngine` with clean API like MeterProcessor:
     - `new()` - Initialize with shared data references
     - `update()` - Process FFT analysis (called from UI update loop)
     - `get_spectrum_data()` - Return processed frequency data for rendering
   - UI component becomes pure display: `SpectrumDisplay`
   - Separate frequency analysis from rendering completely

2. **Future: Audio Engine Refinement**
   - Rename `audio_processor` → `audio_engine`
   - Clarify core effects processing role

3. **Future: Display Component Cleanup**
   - Rename UI components with `_display` suffix
   - Ensure pure rendering responsibility

### Benefits:
- **Clear Separation**: Audio logic vs UI rendering
- **Reusable Components**: Engines can be used by multiple displays
- **Easier Testing**: Test audio logic independently
- **Better Naming**: Purpose is obvious from component names

### ✅ Successful MeterProcessor Pattern (Template for Future Engines):

```rust
// audio/meter_processor.rs - Pure audio processing logic
pub struct MeterProcessor {
    // Thread-safe data references
    peak_level_left: Arc<AtomicF32>,
    peak_level_right: Arc<AtomicF32>,
    
    // Internal processing state
    smoothed_left: f32,
    smoothed_right: f32,
    peak_hold_value: f32,
    silence_counter: u32,
}

impl MeterProcessor {
    pub fn new(peak_left: Arc<AtomicF32>, peak_right: Arc<AtomicF32>) -> Self { /* */ }
    pub fn update(&mut self) { /* All smoothing, peak hold logic */ }
    pub fn get_smoothed_levels(&self) -> (f32, f32) { /* Clean API */ }
    pub fn get_peak_hold_db(&self) -> f32 { /* Clean API */ }
}

// ui/meter.rs - Pure rendering logic
pub struct LevelMeter {
    meter_processor: Arc<MeterProcessor>, // Reference to engine
}

impl<Message> Program<Message, Theme> for LevelMeter {
    fn draw(&self, /* ... */) -> Vec<Geometry> {
        // 1. Update processing engine
        self.meter_processor.update();
        
        // 2. Get processed data via clean API
        let (left, right) = self.meter_processor.get_smoothed_levels();
        
        // 3. Pure rendering - no audio logic
        self.draw_level_bars(frame, size, left, right);
    }
}
```

### Key Patterns for Engine Extraction:
- **Arc<T>** for shared ownership between audio thread and UI
- **Clean APIs**: Simple getter methods, no internal state exposure
- **Update Pattern**: Engine handles all processing in `update()` method
- **Separation**: UI calls engine methods but never implements audio logic

---

**Remember**: This is a learning project. Take time to understand each component, experiment with variations, and don't hesitate to dive deep into the NIH-plug and iced source code to understand how things work under the hood.