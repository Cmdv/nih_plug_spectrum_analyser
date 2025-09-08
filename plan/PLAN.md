# Spectrum Analyzer Implementation Plan for NIH-plug Gain Plugin

## 🎯 Project Goals
- Learn Rust audio processing with NIH-plug
- Implement real-time spectrum analyzer (like EQ plugins)
- Create custom UI with animations using Iced
- Build reusable components for future audio plugins
- Understand thread-safe audio data handling
- Learn FFT and frequency domain analysis

## 🎓 Learning Approach
**Important**: This is a guided learning project. Instead of copying code:
1. **Understand each concept** before implementing
2. **Write the code yourself** with guidance
3. **Ask questions** about anything unclear
4. **Experiment** with variations to deepen understanding
5. **Debug issues** to learn problem-solving

## 📚 Learning Resources
- [NIH-plug Documentation](https://github.com/robbert-vdh/nih-plug)
- [Iced GUI Framework](https://github.com/iced-rs/iced)
- [Real-time Audio Programming Best Practices](https://www.rossbencina.com/code/real-time-audio-programming-101-time-waits-for-nothing)
- [The Rust Programming Language Book](https://doc.rust-lang.org/book/)

## 🏗️ Architecture Overview

### Threading Model
```
Audio Thread (Real-time)          UI Thread (Non-real-time)
    │                                  │
    ├─ process()                      ├─ Iced Event Loop
    │   └─ Write audio samples        │   ├─ Read buffer
    │       to lock-free buffer       │   ├─ Draw waveform
    │                                  │   └─ Handle user input
    └─ No allocations allowed         └─ Can allocate freely
```

### Data Flow
1. **Audio Input** → Plugin process() method
2. **Ring Buffer** → Lock-free circular buffer (triple_buffer crate)
3. **UI Thread** → Reads buffer, performs FFT for frequency analysis
4. **FFT Processing** → Convert time-domain to frequency bins
5. **Canvas** → Draws spectrum bars with Iced graphics
6. **Animation** → 60 FPS refresh with smoothing/decay

## 📦 Dependencies to Add

```toml
[dependencies]
# Core NIH-plug with Iced support
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", features = ["assert_process_allocs"] }
nih_plug_iced = { git = "https://github.com/robbert-vdh/nih-plug.git" }

# Thread-safe audio buffering (lock-free triple buffer)
triple_buffer = "8.0"

# Atomic operations for thread-safe values
atomic_float = "0.1"

# Optional: For spectrum analyzer later
realfft = "3.3"  # FFT for frequency analysis
apodize = "1.0"  # Window functions for FFT
```

## 🎨 UI Components Plan

### 1. Spectrum Analyzer Component
```rust
// Key concepts to implement:
- FFT buffer (typically 1024, 2048, or 4096 samples)
- Window function (Hann, Blackman-Harris) to reduce spectral leakage
- FFT processing to get frequency bins
- Logarithmic frequency scaling (20Hz to 20kHz)
- Magnitude calculation and dB conversion
- Smoothing/averaging for stable display
- Smooth spectrum curve visualization (continuous line)
- Optional fill gradient below the curve
```

### 2. Level Meter
```rust
// Features:
- Peak and RMS levels
- Smooth decay animation
- Color gradients (green → yellow → red)
- dB scale markings
```

### 3. Custom Gain Knob
```rust
// Design elements:
- Radial gradient background
- Smooth rotation animation
- Value display with dB formatting
- Mouse wheel + drag support
```

## 🔧 Implementation Steps

### Phase 1: Basic Setup ✅ COMPLETED
- [x] Analyze existing NIH-plug gain example
- [x] Add triple_buffer dependency to Cargo.toml
- [x] Create basic plugin structure

### Phase 2: Audio Buffer System ✅ COMPLETED
- [x] Implement triple buffer for audio data
- [x] Create `WaveformBuffer` struct with:
  - Fixed-size circular buffer (2048 samples)
  - Write method (audio thread) - writes samples without allocation
  - Read method (UI thread) - returns cloned Vec<f32>
- [x] Add buffer to plugin struct (using Arc<Mutex<WaveformBuffer>>)
- [x] Hook into process() method - collecting mono mix of stereo channels
- [x] Test build passes (with expected warnings for unused consumer/read_samples)

### Phase 3: FFT Setup ✅ COMPLETED
- [x] Add realfft and apodize dependencies
- [x] Create FFT processor struct
- [x] Implement window function (Hann)
- [x] Setup FFT plan with appropriate size (2048)
- [x] Convert buffer to frequency domain

### Phase 4: Spectrum Display ← CURRENT
- [x] Create `src/ui/` folder structure
- [ ] Create `SpectrumView` Iced widget using `iced::widget::canvas`
- [ ] Implement frequency bin to pixel mapping
- [ ] Add logarithmic frequency scaling
- [ ] Draw smooth spectrum curve with canvas
- [ ] Add 60 FPS refresh timer

### Phase 5: Enhanced Visualization
- [ ] Add magnitude smoothing/averaging
- [ ] Implement curve interpolation for smoother lines
- [ ] Add gradient fill below curve
- [ ] Grid lines for frequency/dB reference

### Phase 5: UI Controls
- [ ] Custom gain knob widget
- [ ] Parameter binding to NIH-plug params
- [ ] Smooth parameter changes
- [ ] Visual feedback on interaction

### Phase 6: Styling & Polish
- [ ] Dark theme with gradients
- [ ] Smooth animations
- [ ] Responsive layout
- [ ] Add grid/scale markings

## 💻 Code Structure

```
src/
├── lib.rs                 # Main plugin implementation ✅
├── constants.rs           # Shared constants ✅
├── audio/                 # Audio processing modules ✅
│   ├── mod.rs            # Module exports
│   ├── buffer.rs         # Audio buffer management
│   ├── fft.rs            # FFT processor and frequency analysis
│   └── processor.rs      # Audio processing logic
├── ui/                    # UI components ✅
│   ├── mod.rs            # Module exports
│   ├── spectrum.rs       # Spectrum analyzer widget (IN PROGRESS)
│   ├── knob.rs           # Custom knob widget (TO CREATE)
│   └── style/            # UI styling (TO CREATE)
│       ├── mod.rs        # Style module exports
│       └── theme.rs      # Color schemes and styling
└── editor.rs              # Iced editor setup (TO CREATE)
```

## 🎯 Key Rust Concepts to Learn

1. **Ownership & Borrowing**
   - Arc for shared ownership between threads
   - Mutex/RwLock vs lock-free structures

2. **Thread Safety**
   - Send + Sync traits
   - Atomic operations
   - Lock-free data structures

3. **Real-time Constraints**
   - No allocations in audio thread
   - Bounded execution time
   - Cache-friendly data structures

4. **GUI Event Handling**
   - Message passing pattern
   - State management
   - Reactive updates

## 🚀 Advanced Features (Future)

1. **Spectrum Analyzer**
   - FFT implementation
   - Frequency bins display
   - Logarithmic scaling

2. **Oscilloscope Mode**
   - XY display
   - Trigger detection
   - Multiple channel overlay

3. **Custom Shaders**
   - GPU-accelerated rendering
   - Glow effects
   - Particle systems

## 🐛 Common Pitfalls to Avoid

1. **Audio Thread Violations**
   - Never allocate in process()
   - Don't use mutex locks
   - Avoid unbounded operations

2. **Buffer Overruns**
   - Always check buffer bounds
   - Handle wrap-around correctly
   - Use power-of-2 sizes for efficiency

3. **UI Performance**
   - Don't redraw unnecessarily
   - Cache computed values
   - Use damage tracking

## 📝 Session Notes

### Session 1 
- Explored NIH-plug structure
- Decided on Iced for better waveform visualization
- Created this planning document

### Session 2
- ✅ Implemented complete buffer system in `src/audio/buffer.rs`
  - Triple buffer with producer/consumer split
  - Lock-free communication between threads
  - 2048 sample circular buffer
- ✅ Integrated buffer into main plugin (`src/lib.rs`)
  - Added Arc<Mutex<WaveformBuffer>> to plugin struct
  - Hooked into process() method
  - Collecting mono mix of stereo channels
  - Applying gain AFTER capturing original signal for visualization
- ✅ Learned about Rust references (&T vs T), ownership, and type annotations

### Session 3
- ✅ Reorganized code structure into `audio/` and `ui/` modules
- ✅ Implemented FFT processing in `audio/fft.rs`
- ✅ Added audio processor for managing the processing pipeline
- ✅ Set up UI folder structure with `ui/spectrum.rs`

### Session 4
- Created `ui/` folder structure for Iced widgets
- Researched nih_plug_iced documentation
- Decided to use standard Iced canvas widget for spectrum analyzer
- Working on `SpectrumView` widget implementation

### Session 5
- ✅ Removed `nih_plug_iced` dependency - using Iced directly for canvas support
- ✅ Added `iced = { version = "0.12", features = ["canvas"] }` to Cargo.toml
- ✅ Created `SpectrumView` struct with public fields in `ui/spectrum.rs`
- ✅ Implemented `Program<(), iced::Theme>` trait for SpectrumView with basic draw method
- ✅ Created `editor.rs` with `PluginEditor` struct
- ✅ Implemented NIH-plug `Editor` trait for PluginEditor (basic methods)
- ✅ Changed `AudioProcessor` to use `Arc<Mutex<>>` for thread-safe sharing
- ✅ Connected editor to plugin via `editor()` method in lib.rs
- 🚧 Need to implement `spawn()` method to create actual Iced window
- 🚧 Need to implement Iced `Application` trait properly (currently has todo!())

### Session 6
- ✅ Updated local `nih-plug` fork to iced 0.13 with Canvas support
- ✅ Modified `nih_plug_iced` to use BillyDM/iced_baseview (supports iced 0.13)
- ✅ Canvas feature is available through feature flag chain:
  - Project → `nih_plug_iced` (features = ["canvas"])
  - `nih_plug_iced` → `iced_baseview/canvas`
  - `iced_baseview` → `iced_widget/canvas`
- 📝 **To use Canvas with nih_plug_iced:**
  ```toml
  # In Cargo.toml
  nih_plug = { path = "../nih-plug", features = ["assert_process_allocs"] }
  nih_plug_iced = { path = "../nih-plug/nih_plug_iced", features = ["canvas"] }
  ```
  ```rust
  // In your code
  use nih_plug_iced::widget::canvas::{Canvas, Frame, Geometry, Path, Program};
  ```
- 📝 **Local dependencies compile automatically** - no need to pre-build, Cargo handles it

### Architecture Decisions
- **UI Framework**: Switch back to `nih_plug_iced` (now with iced 0.13 + Canvas support)
- **Canvas Access**: Through `nih_plug_iced::widget::canvas` module
- **Thread Safety**: `AudioProcessor` wrapped in `Arc<Mutex<>>` for sharing between audio and UI threads
- **Parameter Updates**: Will use `GuiContext` and `ParamSetter` for bidirectional parameter communication
- **Spectrum Data**: Planning to pass FFT data from processor to UI via shared Arc

### Session 8-9: Layout System & Dead Space Fixes ✅ COMPLETED

## Iced Layout System (Like CSS Flexbox)

Yes, you're absolutely right! Iced's layout works very much like **CSS Flexbox**:

### Our Current Layout Structure:
```
┌─────────────────────────────────────────────────────────────┐
│ container() - ROOT FLEX CONTAINER                           │
│ .style(container::dark) - Dark background                   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ row![] - HORIZONTAL FLEXBOX (flex-direction: row)  │    │
│  │ .spacing(0) - No gap between flex items            │    │
│  │                                                     │    │
│  │ ┌──────────────────────────┐ ┌─────────────────┐   │    │
│  │ │ SPECTRUM CONTAINER       │ │ RIGHT PANEL     │   │    │
│  │ │ .width(Length::Fill)     │ │ .width(80px)    │   │    │
│  │ │ flex: 1 (takes remain-   │ │ flex: 0 0 80px  │   │    │
│  │ │ ing space after 80px)    │ │ (fixed width)   │   │    │
│  │ │                          │ │                 │   │    │
│  │ │ ┌──────────────────────┐ │ │ ┌─────────────┐ │   │    │
│  │ │ │ Canvas<SpectrumView> │ │ │ │ column![]   │ │   │    │
│  │ │ │ (Spectrum Analyzer)  │ │ │ │ VERTICAL    │ │   │    │
│  │ │ │                      │ │ │ │ FLEXBOX     │ │   │    │
│  │ │ └──────────────────────┘ │ │ │             │ │   │    │
│  │ └──────────────────────────┘ │ │ ┌─────────┐ │ │   │    │
│  │                              │ │ │ Knob    │ │ │   │    │
│  │                              │ │ │ (60×60) │ │ │   │    │
│  │                              │ │ └─────────┘ │ │   │    │
│  │                              │ │ ┌─────────┐ │ │   │    │
│  │                              │ │ │ Meter   │ │ │   │    │
│  │                              │ │ │ (60×∞)  │ │ │   │    │
│  │                              │ │ └─────────┘ │ │   │    │
│  │                              │ └─────────────┘ │   │    │
│  │                              └─────────────────┘   │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### CSS Flexbox Equivalent:
```css
.root-container {
  display: flex;
  width: 100%;
  height: 100vh;
  background: dark;
}

.main-row {
  display: flex;
  flex-direction: row;
  gap: 0;
  width: 100%;
  height: 100%;
}

.spectrum-container {
  flex: 1; /* Takes all remaining space */
  /* Same as: flex-grow: 1, flex-shrink: 1, flex-basis: auto */
}

.right-panel {
  flex: 0 0 80px; /* Fixed 80px width, no grow/shrink */
  display: flex;
  flex-direction: column;
}

.knob { width: 60px; height: 60px; }
.meter { width: 60px; flex: 1; } /* Takes remaining vertical space */
```

### Key Layout Concepts We Used:

1. **`Length::Fill` = `flex: 1`** - Takes all remaining space
2. **`Length::Fixed(80.0)` = `flex: 0 0 80px`** - Fixed size, no flex
3. **`row![]` = `flex-direction: row`** - Horizontal layout
4. **`column![]` = `flex-direction: column`** - Vertical layout
5. **`.spacing(0)` = `gap: 0`** - No space between flex items

### Why Our Fix Worked:
```
BEFORE: FillPortion(6) : FillPortion(1) = 85.7% : 14.3%
  ↓ Problem: 14.3% was too much for 60px knob + 60px meter

AFTER: Fill : Fixed(80px) = (100% - 80px) : 80px  
  ✅ Solution: Spectrum gets ALL remaining space, right panel exactly sized
```

This is exactly like CSS Flexbox - very intuitive once you map the concepts!

## 🎨 Visual Design Ideas

### Color Palette
```rust
// Dark theme with neon accents
const BACKGROUND: Color = Color::from_rgb(0.08, 0.08, 0.12);
const WAVEFORM: Color = Color::from_rgb(0.2, 0.8, 1.0);  // Cyan
const ACCENT: Color = Color::from_rgb(1.0, 0.3, 0.5);    // Pink
const GRID: Color = Color::from_rgba(0.3, 0.3, 0.4, 0.3);
```

### Animation Timing
- Waveform refresh: 60 FPS (16.67ms)
- Parameter smoothing: 20ms
- Peak decay: 300ms
- Hover effects: 150ms

## 📚 Study Materials

1. **Lock-free Programming**
   - [Triple Buffer Crate Docs](https://docs.rs/triple_buffer/)
   - Understanding memory ordering

2. **Digital Signal Processing**
   - Nyquist theorem
   - Window functions
   - RMS vs Peak detection

3. **Iced Framework**
   - Custom widget creation
   - Canvas API
   - Event handling

## 🔗 Useful Commands

### Build and Install Plugin
```bash
# Build the plugin
cargo xtask bundle plugin-learn --release

# Install plugin to system CLAP directory (macOS)
sudo cp -r ./target/bundled/plugin-learn.clap /Library/Audio/Plug-Ins/CLAP

# Remove existing plugin (if updating)
sudo rm -rf /Library/Audio/Plug-Ins/CLAP/plugin-learn.clap
```

### Debug and Logging
```bash
# Launch Bitwig with NIH-plug logging enabled
NIH_LOG=/Users/cmdv/Library/Logs/Bitwig/nih.log ./Applications/Bitwig\ Studio.app/Contents/MacOS/BitwigStudio

# Watch log file in real-time (in separate terminal)
tail -f /Users/cmdv/Library/Logs/Bitwig/nih.log

# Check for audio thread allocations
cargo build --features assert_process_allocs

# Profile performance
cargo build --profile profiling
```

## 💡 Tips for Learning

1. Start simple - get a basic line drawing first
2. Test with sine waves for predictable patterns
3. Use println! debugging (but not in audio thread!)
4. Read other NIH-plug examples for patterns
5. Ask questions in NIH-plug Discord

---

**Remember**: Real-time audio is hard! Take it step by step, and don't be afraid to experiment. The worst that happens is audio glitches, not system crashes.
