//! Conversion from `SocketError` to `ErrorReport` for user-facing error display.

use crate::transport::SocketError;
use veil_core::error::{ErrorComponent, ErrorReport, ErrorSeverity, RecoveryAction};

impl From<SocketError> for ErrorReport {
    fn from(err: SocketError) -> Self {
        match &err {
            SocketError::Io(io_err) => {
                ErrorReport::new(ErrorSeverity::Error, ErrorComponent::Socket, err.to_string())
                    .with_detail(format!("I/O error: {io_err}"))
                    .with_suggestion("check that no other Veil instance is running")
                    .with_recovery_actions(vec![RecoveryAction::Retry, RecoveryAction::Dismiss])
            }
            SocketError::UnsupportedPlatform => {
                ErrorReport::new(ErrorSeverity::Warning, ErrorComponent::Socket, err.to_string())
                    .with_suggestion("the socket API is not available on this platform")
                    .with_recovery_actions(vec![RecoveryAction::Dismiss])
            }
        }
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
        let io_err = std::io::Error::other("test");
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
        let io_err = std::io::Error::other("connection refused");
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
