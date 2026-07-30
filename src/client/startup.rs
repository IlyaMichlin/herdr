use super::*;

/// Runs the thin client and enters the main event loop.
pub fn run_client() -> io::Result<()> {
    run_client_with_mode(None, None, "connecting to server")
}

#[cfg(unix)]
pub fn run_terminal_attach(terminal_id: String, takeover: bool) -> io::Result<()> {
    run_client_with_mode(
        Some(AttachRequest::TerminalId {
            terminal_id,
            takeover,
        }),
        Some(AttachEscapeState::default()),
        "attaching to terminal",
    )
}

/// Runs a direct attach client that resolves its target on the server.
///
/// Unlike [`run_terminal_attach`], this does not need the JSON API socket to
/// turn a fuzzy target (agent name, pane id, terminal id) into a terminal id.
/// `herdr --remote --attach` uses this because it bridges only the client
/// socket, not the API socket.
#[cfg(unix)]
pub fn run_target_attach(target: String, takeover: bool) -> io::Result<()> {
    run_client_with_mode(
        Some(AttachRequest::Target { target, takeover }),
        Some(AttachEscapeState::default()),
        "attaching to terminal target",
    )
}

#[cfg(windows)]
pub fn run_terminal_attach(_terminal_id: String, _takeover: bool) -> io::Result<()> {
    debug_assert!(!crate::platform::capabilities().direct_terminal_attach);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "direct terminal attach is not supported on Windows yet",
    ))
}

/// Server-resolved direct attach follows the same Windows limitation as [`run_terminal_attach`].
#[cfg(windows)]
pub fn run_target_attach(_target: String, _takeover: bool) -> io::Result<()> {
    debug_assert!(!crate::platform::capabilities().direct_terminal_attach);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "direct terminal attach is not supported on Windows yet",
    ))
}
