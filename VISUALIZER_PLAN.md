# Audio Visualizer Implementation Plan for NIH-plug Gain Plugin

## 🎯 Project Goals
- Learn Rust audio processing with NIH-plug
- Implement real-time waveform visualization
- Create custom UI with animations using Iced
- Build reusable components for future audio plugins
- Understand thread-safe audio data handling

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
3. **UI Thread** → Reads buffer, calculates waveform points
4. **Canvas** → Draws waveform with Iced graphics
5. **Animation** → 60 FPS refresh for smooth visualization

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

### 1. Waveform Display Component
```rust
// Key concepts to implement:
- Circular buffer to store recent audio samples
- Downsampling for display (e.g., 48kHz audio → 60Hz display)
- Peak detection for better visualization
- Smooth interpolation between points
- Configurable colors and stroke width
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

### Phase 1: Basic Setup ✅ Current
- [x] Analyze existing NIH-plug gain example
- [ ] Add Iced dependencies to Cargo.toml
- [ ] Create basic plugin editor structure

### Phase 2: Audio Buffer System
- [ ] Implement triple buffer for audio data
- [ ] Create `WaveformBuffer` struct with:
  - Fixed-size circular buffer
  - Write method (audio thread)
  - Read method (UI thread)
- [ ] Add buffer to plugin struct

### Phase 3: Basic Waveform Display
- [ ] Create `WaveformView` Iced widget
- [ ] Implement canvas drawing with basic line
- [ ] Connect to audio buffer
- [ ] Add 60 FPS refresh timer

### Phase 4: Enhanced Visualization
- [ ] Add peak detection algorithm
- [ ] Implement smooth interpolation
- [ ] Add fade effect for older samples
- [ ] Create gradient fill option

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
├── lib.rs                 # Main plugin implementation
├── editor.rs              # Iced editor setup
├── buffer.rs              # Audio buffer management
├── widgets/
│   ├── mod.rs
│   ├── waveform.rs        # Waveform display widget
│   ├── level_meter.rs     # VU/Peak meter widget
│   └── knob.rs            # Custom knob widget
└── style/
    ├── mod.rs
    └── theme.rs           # Color schemes and styling
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

### Session 1 (Current)
- Explored NIH-plug structure
- Decided on Iced for better waveform visualization
- Created this planning document
- Ready to implement basic buffer system

### Next Session Tasks
1. Add dependencies to Cargo.toml
2. Create `buffer.rs` with `WaveformBuffer` struct
3. Implement basic Iced editor
4. Test with simple waveform drawing

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

```bash
# Build the plugin
cargo xtask bundle plugin_learn --release

# Run with logging
RUST_LOG=debug cargo run

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