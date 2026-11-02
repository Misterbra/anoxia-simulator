//! Error types of the anoxia-core library.

use thiserror::Error;

/// Result type for anoxie operations.
pub type Result<T> = std::result::Result<T, AnoxiaError>;

/// Errors that can occur while configuring or running a simulation.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AnoxiaError {
    /// A configuration value is outside its physically valid range.
    #[error("{field}: {value} is out of range [{min}, {max}] {unit}")]
    OutOfRange {
        /// Name of the field
        field: &'static str,
        /// Rejected value
        value: f64,
        /// Minimum accepted value
        min: f64,
        /// Maximum accepted value
        max: f64,
        /// Unit of the value
        unit: &'static str,
    },

    /// Inconsistent configuration (combination of values that cannot be simulated).
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// Serialization / deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(String),
}

impl From<std::io::Error> for AnoxiaError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AnoxiaError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Check that `value` is finite and within `[min, max]`.
///
/// # Errors
/// Returns [`AnoxiaError::OutOfRange`] otherwise (NaN and infinities are rejected).
pub fn check_range(field: &'static str, value: f64, min: f64, max: f64, unit: &'static str) -> Result<()> {
    if value.is_finite() && value >= min && value <= max {
        Ok(())
    } else {
        Err(AnoxiaError::OutOfRange { field, value, min, max, unit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_check_rejects_nan_and_bounds() {
        assert!(check_range("x", 1.0, 0.0, 2.0, "m").is_ok());
        assert!(check_range("x", f64::NAN, 0.0, 2.0, "m").is_err());
        assert!(check_range("x", f64::INFINITY, 0.0, 2.0, "m").is_err());
        assert!(check_range("x", -0.1, 0.0, 2.0, "m").is_err());
        assert!(check_range("x", 2.1, 0.0, 2.0, "m").is_err());
    }

    #[test]
    fn error_message_is_explicit() {
        let e = check_range("room_volume", -1.0, 0.1, 1e6, "m³").unwrap_err();
        assert!(e.to_string().contains("room_volume"));
        assert!(e.to_string().contains("m³"));
    }
}
