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

## 🚀 Session 9 Goals

1. **Implement Circular Knob Widget**
   - Full mouse interaction (drag to rotate)
   - Visual feedback (hover states)
   - Proper parameter binding
   - Pro-Q style appearance

2. **Add Text Rendering**
   - Frequency labels on spectrum
   - dB values on meters
   - Gain value on knob

3. **Polish Visual Design**
   - Fine-tune colors to match Pro-Q
   - Add subtle animations
   - Improve grid appearance

4. **Optional Enhancements**
   - Pre/post gain spectrum comparison
   - Peak hold indicators on meters
   - Preset system

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

**Remember**: This is a learning project. Take time to understand each component, experiment with variations, and don't hesitate to dive deep into the NIH-plug and iced source code to understand how things work under the hood.