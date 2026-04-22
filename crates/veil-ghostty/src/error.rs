//! Error types for libghosty FFI operations.

/// Errors from libghosty FFI operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GhosttyError {
    /// Memory allocation failed inside libghosty.
    #[error("libghosty allocation failed")]
    OutOfMemory,

    /// An invalid value was passed to or returned from libghosty.
    #[error("invalid value in libghosty call")]
    InvalidValue,

    /// A provided buffer was too small.
    #[error("buffer too small for libghosty output")]
    OutOfSpace,

    /// The requested value has no value (e.g., unset optional color).
    #[error("requested value is not set")]
    NoValue,

    /// A panic was caught at the FFI boundary.
    #[error("panic caught at FFI boundary")]
    Panic,

    /// An unexpected/unknown result code was returned.
    #[error("unknown libghosty error code: {0}")]
    Unknown(i32),
}

/// Convert a raw `GhosttyResult` (C `int`) to `Result<(), GhosttyError>`.
///
/// `GHOSTTY_SUCCESS` (0) maps to `Ok(())`, all other codes map to the
/// corresponding error variant.
pub(crate) fn check_result(code: i32) -> Result<(), GhosttyError> {
    match code {
        0 => Ok(()),
        -1 => Err(GhosttyError::OutOfMemory),
        -2 => Err(GhosttyError::InvalidValue),
        -3 => Err(GhosttyError::OutOfSpace),
        -4 => Err(GhosttyError::NoValue),
        other => Err(GhosttyError::Unknown(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- check_result mapping tests ----

    #[test]
    fn check_result_success_returns_ok() {
        assert_eq!(check_result(0), Ok(()));
    }

    #[test]
    fn check_result_out_of_memory() {
        assert_eq!(check_result(-1), Err(GhosttyError::OutOfMemory),);
    }

    #[test]
    fn check_result_invalid_value() {
        assert_eq!(check_result(-2), Err(GhosttyError::InvalidValue),);
    }

    #[test]
    fn check_result_out_of_space() {
        assert_eq!(check_result(-3), Err(GhosttyError::OutOfSpace),);
    }

    #[test]
    fn check_result_no_value() {
        assert_eq!(check_result(-4), Err(GhosttyError::NoValue),);
    }

    #[test]
    fn check_result_unknown_code_maps_to_unknown() {
        assert_eq!(check_result(99), Err(GhosttyError::Unknown(99)),);
    }

    // ---- Display / Error trait tests ----

    #[test]
    fn error_display_messages_are_meaningful() {
        assert_eq!(GhosttyError::OutOfMemory.to_string(), "libghosty allocation failed",);
        assert_eq!(GhosttyError::InvalidValue.to_string(), "invalid value in libghosty call",);
        assert_eq!(GhosttyError::OutOfSpace.to_string(), "buffer too small for libghosty output",);
        assert_eq!(GhosttyError::NoValue.to_string(), "requested value is not set",);
        assert_eq!(GhosttyError::Panic.to_string(), "panic caught at FFI boundary",);
        assert_eq!(GhosttyError::Unknown(42).to_string(), "unknown libghosty error code: 42",);
    }

    // ---- Send + Sync tests ----

    #[test]
    fn ghostty_error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<GhosttyError>();
        assert_sync::<GhosttyError>();
    }
}
