# Plugin Learn - Audio Visualizer Project

## Project Overview
This is a NIH-plug based audio gain plugin with real-time waveform visualization using Iced GUI framework. The project is focused on learning Rust, audio processing, and real-time graphics.

## Learning Mode
**IMPORTANT**: This is a learning project. When helping with this codebase:
- **Guide, don't implement** - Show what needs to be done, explain concepts
- **Explain Rust patterns** - Especially ownership, borrowing, and thread safety
- **Highlight audio programming constraints** - Real-time considerations
- **Encourage exploration** - Suggest experiments and variations

## Current Status
- Basic gain plugin structure exists (src/lib.rs)
- Planning to add Iced-based UI with waveform visualization
- Learning focus: thread-safe audio buffering and real-time graphics

## Architecture Decisions
- **UI Framework**: Iced (chosen for smooth animations and waveform rendering)
- **Buffer Strategy**: Triple buffer for lock-free audio/UI communication
- **Update Rate**: 60 FPS for smooth visualization

## Key Files
- `src/lib.rs` - Main plugin implementation (WITH BUFFER INTEGRATED)
- `PLAN.md` - **CRITICAL: Read this for current progress and next steps**
- `src/buffer.rs` - Audio buffer management (COMPLETED)
- `src/editor.rs` - (To create) Iced UI implementation

## Commands
```bash
# Build the plugin
cargo xtask bundle plugin_learn --release

# Test build
cargo build

# Check for audio thread allocations
cargo build --features assert_process_allocs
```

## Teaching Guidelines
When asked for help:
1. First explain the concept
2. Show code structure/skeleton
3. Explain each part's purpose
4. Highlight potential issues
5. Suggest experiments

**CRITICAL**: NEVER make up functions or methods. Always verify they exist in the documentation or codebase. If unsure, ask the user or suggest they check the docs. Making up APIs wastes learning time and causes frustration.

## Current Learning Topics
- Lock-free data structures (triple_buffer)
- Real-time audio constraints
- Iced custom widgets
- Thread communication in Rust

## Session Context
User is learning Rust/NIH-plug/audio processing. They prefer to write code themselves with guidance rather than having code written for them. Focus on teaching and explaining.

## IMPORTANT FOR NEW SESSIONS
**ALWAYS READ `PLAN.md` FIRST** - It contains:
- Current implementation status (what's completed vs pending)
- Session notes showing exactly where we left off
- Next tasks to work on
- Architecture decisions and learning goals
