pub mod grid_overlay;
pub mod meter_display;
pub mod spectrum_display;
pub mod style;
pub mod shaders;  // Our new WGPU shaders

pub use grid_overlay::GridOverlay;
pub use meter_display::MeterDisplay;
pub use spectrum_display::SpectrumDisplay;
pub use style::UITheme;
pub use shaders::GridShader;  // Re-export for easy access
