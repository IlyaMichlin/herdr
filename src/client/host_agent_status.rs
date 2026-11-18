//! Mirrors a directly attached terminal's agent status into the host terminal.
//!
//! A direct-attach client is the only local process in the host terminal tab,
//! so host integrations that rely on agent hooks (for example the cmux sidebar)
//! never hear from an agent that runs inside a Herdr server, especially a
//! remote one. The server streams the attached terminal's effective agent state
//! to the client, and this module turns state transitions into host commands.
//!
//! cmux is the only supported host today. It is detected from the environment
//! cmux sets for every terminal surface, and driven through its public CLI.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread::JoinHandle;

use tracing::{debug, warn};

use crate::protocol::endpoint::{DirectTerminalAgentState, DirectTerminalAgentStatus};

/// Bounds each cmux CLI call so a wedged cmux socket cannot stall detach.
const CMUX_CLI_TIMEOUT_SECONDS: &str = "2";

/// Sidebar status key owned by Herdr. cmux reserves agent keys such as
/// `claude_code` for its own wrapper processes and drops them for other
/// callers, so Herdr keeps a separate pill.
const STATUS_KEY: &str = "herdr_agent";

const RUNNING_ICON: &str = "bolt.fill";
const RUNNING_COLOR: &str = "#4C8DFF";
const IDLE_ICON: &str = "pause.circle.fill";
const IDLE_COLOR: &str = "#8E8E93";
const BLOCKED_ICON: &str = "bell.fill";
const BLOCKED_COLOR: &str = "#FF9500";

/// One host-side effect of an agent status transition.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostAction {
    SetStatus {
        value: String,
        icon: &'static str,
        color: &'static str,
    },
    ClearStatus,
    Notify {
        title: String,
        body: &'static str,
    },
    ClearNotifications,
}

/// cmux surface that owns this client's host terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CmuxTarget {
    cli: PathBuf,
    socket: Option<String>,
    workspace_id: Option<String>,
    surface_id: String,
}

impl CmuxTarget {
    fn from_env(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let non_empty = |name: &str| get(name).filter(|value| !value.trim().is_empty());
        if non_empty("CMUX_CLAUDE_INTEGRATION_DISABLED").as_deref() == Some("1") {
            return None;
        }
        let surface_id = non_empty("CMUX_SURFACE_ID")?;
        let cli = non_empty("CMUX_BUNDLED_CLI_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("cmux"));
        Some(Self {
            cli,
            socket: non_empty("CMUX_SOCKET_PATH"),
            workspace_id: non_empty("CMUX_WORKSPACE_ID"),
            surface_id,
        })
    }

    fn args(&self, action: &HostAction) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();
        if let Some(socket) = &self.socket {
            args.push("--socket".into());
            args.push(socket.into());
        }
        match action {
            HostAction::SetStatus { value, icon, color } => {
                args.extend(["set-status".into(), STATUS_KEY.into(), value.into()]);
                args.extend(["--icon".into(), (*icon).into()]);
                args.extend(["--color".into(), (*color).into()]);
            }
            HostAction::ClearStatus => {
                args.extend(["clear-status".into(), STATUS_KEY.into()]);
            }
            HostAction::Notify { title, body } => {
                args.extend(["notify".into(), "--title".into(), title.into()]);
                args.extend(["--body".into(), (*body).into()]);
                args.extend(["--surface".into(), (&self.surface_id).into()]);
            }
            HostAction::ClearNotifications => {
                args.extend(["notify".into(), "--clear".into()]);
                args.extend(["--surface".into(), (&self.surface_id).into()]);
            }
        }
        // Every command is scoped to this client's workspace; `notify --surface`
        // rejects a surface without workspace context.
        if let Some(workspace_id) = &self.workspace_id {
            args.push("--workspace".into());
            args.push(workspace_id.into());
        }
        args
    }
}

fn agent_title(agent: &str) -> String {
    match agent {
        "claude" => "Claude Code".to_owned(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        }
    }
}

/// The agent a status describes, or `None` when the terminal has no live agent.
fn active_agent(status: &DirectTerminalAgentStatus) -> Option<&str> {
    status
        .agent
        .as_deref()
        .filter(|_| status.state != DirectTerminalAgentState::Unknown)
}

/// Host effects for one status transition. `previous` is `None` for the first
/// status after attaching.
fn plan_transition(
    previous: Option<&DirectTerminalAgentStatus>,
    next: &DirectTerminalAgentStatus,
) -> Vec<HostAction> {
    let mut actions = Vec::new();
    let Some(agent) = active_agent(next) else {
        if previous.and_then(active_agent).is_some() {
            actions.push(HostAction::ClearStatus);
        }
        if previous.is_none() {
            // Drop notifications left on this surface by an earlier session.
            actions.push(HostAction::ClearNotifications);
        }
        return actions;
    };
    let previous_state = previous
        .filter(|previous| active_agent(previous) == Some(agent))
        .map(|previous| previous.state);
    let title = agent_title(agent);
    match next.state {
        DirectTerminalAgentState::Working => {
            actions.push(HostAction::SetStatus {
                value: format!("{title} · Running"),
                icon: RUNNING_ICON,
                color: RUNNING_COLOR,
            });
            if previous_state != Some(DirectTerminalAgentState::Working) {
                actions.push(HostAction::ClearNotifications);
            }
        }
        DirectTerminalAgentState::Idle => {
            actions.push(HostAction::SetStatus {
                value: format!("{title} · Idle"),
                icon: IDLE_ICON,
                color: IDLE_COLOR,
            });
            if previous.is_none() {
                actions.push(HostAction::ClearNotifications);
            } else if previous_state == Some(DirectTerminalAgentState::Working) {
                actions.push(HostAction::Notify {
                    title,
                    body: "Finished",
                });
            }
        }
        DirectTerminalAgentState::Blocked => {
            actions.push(HostAction::SetStatus {
                value: format!("{title} · Needs input"),
                icon: BLOCKED_ICON,
                color: BLOCKED_COLOR,
            });
            if previous.is_some() && previous_state != Some(DirectTerminalAgentState::Blocked) {
                actions.push(HostAction::Notify {
                    title,
                    body: "Needs your input",
                });
            }
        }
        DirectTerminalAgentState::Unknown => {}
    }
    actions
}

/// Applies attached-terminal agent status to the host terminal.
///
/// Host commands run in order on one worker thread so the client loop never
/// waits on the host CLI. Dropping the bridge clears the status it set.
pub(super) struct HostAgentStatus {
    target: CmuxTarget,
    last: Option<DirectTerminalAgentStatus>,
    commands: Option<mpsc::Sender<Vec<OsString>>>,
    worker: Option<JoinHandle<()>>,
}

impl HostAgentStatus {
    /// Returns a bridge when the client runs inside a supported host terminal.
    pub(super) fn from_environment() -> Option<Self> {
        let target = CmuxTarget::from_env(|name| std::env::var(name).ok())?;
        let (commands, receiver) = mpsc::channel::<Vec<OsString>>();
        let cli = target.cli.clone();
        let worker = std::thread::Builder::new()
            .name("herdr-host-agent-status".into())
            .spawn(move || {
                for args in receiver {
                    run_cmux(&cli, &args);
                }
            })
            .map_err(|err| warn!(err = %err, "failed to start host agent status worker"))
            .ok()?;
        debug!(surface = %target.surface_id, "mirroring attached agent status to cmux");
        Some(Self {
            target,
            last: None,
            commands: Some(commands),
            worker: Some(worker),
        })
    }

    /// Applies one wire status payload. Malformed payloads are ignored.
    pub(super) fn apply_wire(&mut self, data: &str) {
        match serde_json::from_str::<DirectTerminalAgentStatus>(data) {
            Ok(status) => self.apply(status),
            Err(err) => debug!(err = %err, "ignoring malformed direct terminal agent status"),
        }
    }

    fn apply(&mut self, status: DirectTerminalAgentStatus) {
        if self.last.as_ref() == Some(&status) {
            return;
        }
        debug!(agent = ?status.agent, state = ?status.state, "attached agent status changed");
        for action in plan_transition(self.last.as_ref(), &status) {
            self.send(&action);
        }
        self.last = Some(status);
    }

    fn send(&self, action: &HostAction) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(self.target.args(action));
        }
    }
}

impl Drop for HostAgentStatus {
    fn drop(&mut self) {
        if self.last.as_ref().and_then(active_agent).is_some() {
            self.send(&HostAction::ClearStatus);
        }
        // Closing the channel lets the worker finish queued commands and exit.
        self.commands = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_cmux(cli: &std::path::Path, args: &[OsString]) {
    let result = Command::new(cli)
        .args(args)
        .env(
            "CMUXTERM_CLI_RESPONSE_TIMEOUT_SEC",
            CMUX_CLI_TIMEOUT_SECONDS,
        )
        .env("CMUX_QUIET", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match result {
        Ok(status) if status.success() => {}
        Ok(status) => debug!(%status, ?args, "cmux command failed"),
        Err(err) => debug!(err = %err, ?args, "failed to run cmux"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use DirectTerminalAgentState::{Blocked, Idle, Unknown, Working};

    fn status(agent: Option<&str>, state: DirectTerminalAgentState) -> DirectTerminalAgentStatus {
        DirectTerminalAgentStatus {
            agent: agent.map(str::to_owned),
            state,
        }
    }

    fn set(value: &str, icon: &'static str, color: &'static str) -> HostAction {
        HostAction::SetStatus {
            value: value.into(),
            icon,
            color,
        }
    }

    #[test]
    fn first_idle_status_sets_pill_and_clears_stale_notifications() {
        assert_eq!(
            plan_transition(None, &status(Some("claude"), Idle)),
            vec![
                set("Claude Code · Idle", IDLE_ICON, IDLE_COLOR),
                HostAction::ClearNotifications
            ]
        );
    }

    #[test]
    fn first_status_without_agent_only_clears_stale_notifications() {
        assert_eq!(
            plan_transition(None, &status(None, Unknown)),
            vec![HostAction::ClearNotifications]
        );
    }

    #[test]
    fn first_blocked_status_does_not_notify() {
        assert_eq!(
            plan_transition(None, &status(Some("claude"), Blocked)),
            vec![set(
                "Claude Code · Needs input",
                BLOCKED_ICON,
                BLOCKED_COLOR
            )]
        );
    }

    #[test]
    fn working_clears_notifications_once() {
        let idle = status(Some("claude"), Idle);
        let working = status(Some("claude"), Working);
        assert_eq!(
            plan_transition(Some(&idle), &working),
            vec![
                set("Claude Code · Running", RUNNING_ICON, RUNNING_COLOR),
                HostAction::ClearNotifications
            ]
        );
        assert_eq!(
            plan_transition(Some(&working), &working),
            vec![set("Claude Code · Running", RUNNING_ICON, RUNNING_COLOR)]
        );
    }

    #[test]
    fn finishing_work_notifies() {
        assert_eq!(
            plan_transition(
                Some(&status(Some("claude"), Working)),
                &status(Some("claude"), Idle)
            ),
            vec![
                set("Claude Code · Idle", IDLE_ICON, IDLE_COLOR),
                HostAction::Notify {
                    title: "Claude Code".into(),
                    body: "Finished",
                }
            ]
        );
    }

    #[test]
    fn becoming_blocked_notifies() {
        assert_eq!(
            plan_transition(
                Some(&status(Some("claude"), Working)),
                &status(Some("claude"), Blocked)
            ),
            vec![
                set("Claude Code · Needs input", BLOCKED_ICON, BLOCKED_COLOR),
                HostAction::Notify {
                    title: "Claude Code".into(),
                    body: "Needs your input",
                }
            ]
        );
    }

    #[test]
    fn agent_exit_clears_the_pill() {
        assert_eq!(
            plan_transition(
                Some(&status(Some("claude"), Idle)),
                &status(Some("claude"), Unknown)
            ),
            vec![HostAction::ClearStatus]
        );
    }

    #[test]
    fn agent_switch_replaces_pill_without_stale_transition() {
        assert_eq!(
            plan_transition(
                Some(&status(Some("claude"), Working)),
                &status(Some("codex"), Idle)
            ),
            vec![set("Codex · Idle", IDLE_ICON, IDLE_COLOR)]
        );
    }

    #[test]
    fn unknown_wire_state_is_treated_as_no_agent() {
        let decoded: DirectTerminalAgentStatus =
            serde_json::from_str(r#"{"agent":"claude","state":"thinking_hard"}"#)
                .expect("unknown state decodes");
        assert_eq!(decoded.state, Unknown);
        assert_eq!(active_agent(&decoded), None);
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn cmux_target_requires_surface() {
        assert_eq!(
            CmuxTarget::from_env(env(&[("CMUX_SOCKET_PATH", "/tmp/cmux.sock")])),
            None
        );
        assert_eq!(
            CmuxTarget::from_env(env(&[
                ("CMUX_SURFACE_ID", "S"),
                ("CMUX_CLAUDE_INTEGRATION_DISABLED", "1"),
            ])),
            None
        );
    }

    #[test]
    fn cmux_args_target_this_surface_and_workspace() {
        let target = CmuxTarget::from_env(env(&[
            ("CMUX_SURFACE_ID", "S"),
            ("CMUX_WORKSPACE_ID", "W"),
            ("CMUX_SOCKET_PATH", "/tmp/cmux.sock"),
            ("CMUX_BUNDLED_CLI_PATH", "/opt/cmux"),
        ]))
        .expect("cmux target");
        assert_eq!(target.cli, PathBuf::from("/opt/cmux"));
        let strings = |action: &HostAction| {
            target
                .args(action)
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            strings(&set("Claude Code · Running", RUNNING_ICON, RUNNING_COLOR)),
            [
                "--socket",
                "/tmp/cmux.sock",
                "set-status",
                STATUS_KEY,
                "Claude Code · Running",
                "--icon",
                RUNNING_ICON,
                "--color",
                RUNNING_COLOR,
                "--workspace",
                "W"
            ]
        );
        assert_eq!(
            strings(&HostAction::ClearNotifications),
            [
                "--socket",
                "/tmp/cmux.sock",
                "notify",
                "--clear",
                "--surface",
                "S",
                "--workspace",
                "W"
            ]
        );
    }
}
