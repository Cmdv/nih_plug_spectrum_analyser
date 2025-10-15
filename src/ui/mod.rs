pub mod meter_display;
pub mod style;
pub mod shaders;  // Our new WGPU shaders

pub use meter_display::MeterDisplay;
pub use style::UITheme;
pub use shaders::GridShader;  // Re-export for easy access
pub use shaders::SpectrumShader;  // Re-export for easy access
