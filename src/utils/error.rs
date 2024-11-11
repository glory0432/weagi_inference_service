use axum::http::StatusCode;
use tracing::error;
pub fn format_error(
    message: &str,
    error: impl std::fmt::Display,
    status: StatusCode,
) -> (StatusCode, String) {
    let error_message = format!("{}: {}", message, error);
    error!("Error occurred: {}", error_message);
    (status, error_message)
}
