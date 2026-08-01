pub mod friends;
pub mod profile;
pub mod sync;

/// Logs the error side of a command's result to the on-disk log file
/// before returning it, so a friend hitting a bug can just hand over the
/// log (via the "Copy diagnostics" button in the UI) instead of having to
/// reproduce it live or transcribe an error message by hand.
pub(crate) fn log_err<T>(command: &str, result: Result<T, String>) -> Result<T, String> {
    if let Err(e) = &result {
        log::error!("{command} failed: {e}");
    }
    result
}
