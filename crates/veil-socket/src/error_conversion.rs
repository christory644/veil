//! Conversion from `SocketError` to `ErrorReport` for user-facing error display.

use crate::transport::SocketError;
use veil_core::error::{ErrorComponent, ErrorReport, ErrorSeverity};

// Stub: returns a default ErrorReport so tests will fail.
impl From<SocketError> for ErrorReport {
    fn from(_err: SocketError) -> Self {
        ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::System, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_socket_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "address in use");
        let err = SocketError::Io(io_err);
        let report: ErrorReport = err.into();
        assert_eq!(report.severity, ErrorSeverity::Error);
        assert_eq!(report.component, ErrorComponent::Socket);
    }

    #[test]
    fn from_socket_unsupported_platform() {
        let err = SocketError::UnsupportedPlatform;
        let report: ErrorReport = err.into();
        assert_eq!(
            report.severity,
            ErrorSeverity::Warning,
            "UnsupportedPlatform should be Warning"
        );
        assert_eq!(report.component, ErrorComponent::Socket);
    }

    #[test]
    fn socket_error_report_has_recovery_actions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let io_report: ErrorReport = SocketError::Io(io_err).into();
        assert!(
            !io_report.recovery_actions.is_empty(),
            "Io error should have at least one recovery action"
        );

        let platform_report: ErrorReport = SocketError::UnsupportedPlatform.into();
        assert!(
            !platform_report.recovery_actions.is_empty(),
            "UnsupportedPlatform should have at least one recovery action"
        );
    }

    #[test]
    fn socket_error_into_report_preserves_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "connection refused");
        let socket_err = SocketError::Io(io_err);
        let display_str = socket_err.to_string();
        let report: ErrorReport = socket_err.into();
        assert!(!report.message.is_empty(), "report message should not be empty");
        assert!(
            report.message.contains(&display_str) || display_str.contains(&report.message),
            "report message '{}' should relate to SocketError display '{}'",
            report.message,
            display_str
        );
    }
}
