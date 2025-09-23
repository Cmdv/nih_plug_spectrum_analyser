# NIH-plug Spectrum Analyzer

A real-time audio spectrum analyzer plugin built with [NIH-plug](https://github.com/robbert-vdh/nih-plug) and [Iced](https://github.com/iced-rs/iced) for the GUI. This plugin provides professional-grade spectrum analysis with adaptive windowing, customizable display options, and real-time metering.

## Features

- **Real-time FFT spectrum analysis** with configurable resolution levels (128-2049 bins)
- **Resolution-adaptive curve smoothing** - Automatic curve radius adjustment based on resolution level
  - Low resolution: Large radius curves for smooth, flowing visualization
  - High resolution: Tight radius curves preserving fine detail
  - Frequency-aware smoothing: Larger curves for low frequencies, detailed high frequencies
- **Adaptive windowing** - Different window functions for low, mid, and high frequencies
- **Lock-free audio/UI communication** using triple buffering
- **Level metering** with peak hold and smoothed decay
- **Perceptual tilt compensation** (+4.5dB/octave) for flatter visual response
- **Catmull-Rom spline rendering** for professional-grade smooth spectrum curves
- **Zero allocation in audio thread** - Real-time safe processing

## Architecture

The plugin follows a clean producer/consumer pattern:
- **Audio thread (Producer)**: Generates spectrum and meter data
- **UI thread (Consumer)**: Reads and displays the data
- **Lock-free communication**: Triple buffering for spectrum, atomic values for meters

## Dependencies

Note: Currently using local paths for NIH-plug. Update these in `Cargo.toml` to point to external repos:
```toml
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git" }
nih_plug_iced = { git = "https://github.com/robbert-vdh/nih-plug.git" }
```

## Building

After installing [Rust](https://rustup.rs/), you can compile the plugin:

```shell
cargo xtask bundle spectrum_analyser --release
```

The compiled plugin will be in `target/bundled/` as:
- `spectrum_analyser.clap` - CLAP format

## Installation Scripts (macOS)

⚠️ **Note**: The provided scripts are configured for macOS and use specific paths. You may need to adjust them for your system.

### Quick Build & Install (`scripts/run.sh`)

This script automates the entire workflow:
1. Builds the plugin in release mode
2. Removes old plugin version from `/Library/Audio/Plug-Ins/CLAP/`
3. Installs the new version
4. Launches Bitwig Studio with logging enabled
5. Opens a tmux session with:
   - Left pane: Bitwig Studio
   - Right pane: Live log monitoring

```shell
./scripts/run.sh
```

### Build Only (`scripts/build.sh`)

Just builds and installs the plugin without launching Bitwig:

```shell
./scripts/build.sh
```

## Logging & Debugging

The plugin uses NIH-plug's logging system. You can add debug output in your code:

```rust
nih_plug::nih_log!("Debug message: value = {}", some_value);
```

When running through `scripts/run.sh`, logs are:
- Written to: `~/Library/Logs/Bitwig/nih.log`
- Displayed in real-time in the tmux right pane
- Enabled via the `NIH_LOG` environment variable

### Manual Logging Setup

To enable logging when running your DAW manually:

```shell
# For Bitwig Studio
NIH_LOG=~/Library/Logs/Bitwig/nih.log "/Applications/Bitwig Studio.app/Contents/MacOS/BitwigStudio"

# For other DAWs, adjust the path accordingly
NIH_LOG=~/my_plugin.log /path/to/your/daw
```

## Platform Notes

- **macOS**: Scripts assume CLAP plugins go in `/Library/Audio/Plug-Ins/CLAP/`
- **Linux**: Adjust paths in scripts, typically `~/.clap/` or `/usr/lib/clap/`
- **Windows**: Adjust paths, typically `C:\Program Files\Common Files\CLAP\`

## Development

This is a learning project focused on:
- Lock-free audio programming patterns
- Real-time safe DSP in Rust
- Custom Iced widgets with Canvas
- Functional programming in systems contexts

## License

TODO: Add license information
