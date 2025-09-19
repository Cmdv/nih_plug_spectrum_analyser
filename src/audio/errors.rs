/// Custom error types for audio processing
use thiserror::Error;

/// Errors that can occur during spectrum analysis
#[derive(Debug, Error)]
#[allow(dead_code)] // These variants are part of the public API for future use
pub enum SpectrumError {
    /// FFT processing failed
    #[error("FFT processing failed")]
    FftFailed,

    /// Failed to acquire lock for thread-safe access
    #[error("Failed to acquire lock for {resource}")]
    LockFailed { resource: String },

    /// Invalid buffer configuration
    #[error("Invalid buffer: {reason}")]
    InvalidBuffer { reason: String },

    /// Window size mismatch
    #[error("Window size mismatch: expected {expected}, got {actual}")]
    WindowSizeMismatch { expected: usize, actual: usize },
}

/// Errors that can occur during metering
#[derive(Debug, Error)]
#[allow(dead_code)] // LockFailed variant is part of the public API for future use
pub enum MeterError {
    /// No audio channels in buffer
    #[error("No audio channels in buffer")]
    NoChannels,

    /// Failed to acquire lock for thread-safe access
    #[error("Failed to acquire lock for meter state")]
    LockFailed,
}

/// Result type for spectrum operations
pub type SpectrumResult<T> = Result<T, SpectrumError>;

/// Result type for meter operations
pub type MeterResult<T> = Result<T, MeterError>;
