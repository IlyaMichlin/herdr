pub(crate) const REMOTE_PREFERENCES_ENV_VAR: &str = "HERDR_REMOTE_PREFERENCES_IDENTITY";
pub(crate) const REATTACH_COMMAND_ENV_VAR: &str = "HERDR_REATTACH_COMMAND";
pub(crate) const REMOTE_KEYBINDINGS_ENV_VAR: &str = "HERDR_REMOTE_KEYBINDINGS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteKeybindings {
    Local,
    Server,
}

impl RemoteKeybindings {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "server" => Ok(Self::Server),
            _ => Err("--remote-keybindings must be 'local' or 'server'".to_string()),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Server => "server",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteLaunch {
    pub(crate) target: String,
    pub(crate) keybindings: RemoteKeybindings,
    pub(crate) live_handoff: bool,
    /// Attach straight to one remote terminal instead of the full app UI.
    pub(crate) attach: Option<RemoteAttach>,
}

/// Direct-attach request for `--remote --attach <target>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteAttach {
    /// Agent name, pane id, or terminal id, resolved by the remote server.
    pub(crate) target: String,
    pub(crate) takeover: bool,
}

pub(crate) fn extract_remote_args(
    args: &[String],
) -> Result<(Vec<String>, Option<RemoteLaunch>), String> {
    let mut cleaned = Vec::with_capacity(args.len());
    if let Some(program) = args.first() {
        cleaned.push(program.clone());
    }

    // `--remote` is only valid for the default launch command, and subcommands
    // own their own flags (`client-attach <target> --takeover`, for example).
    // Hand subcommand argv through untouched so this parser never steals a flag
    // that belongs to the subcommand.
    if args
        .get(1)
        .is_some_and(|arg| !arg.starts_with('-') && arg != "--")
    {
        cleaned.extend_from_slice(&args[1..]);
        return Ok((cleaned, None));
    }

    let mut remote_target = None;
    let mut keybindings = RemoteKeybindings::Local;
    let mut keybindings_seen = false;
    let mut live_handoff = false;
    let mut attach_target = None;
    let mut takeover = false;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            cleaned.extend_from_slice(&args[index..]);
            break;
        }
        if arg == "--handoff" {
            live_handoff = true;
            index += 1;
            continue;
        }
        if arg == "--attach" {
            if attach_target.is_some() {
                return Err("--attach can only be specified once".to_string());
            }
            let Some(value) = args.get(index + 1) else {
                return Err("missing value for --attach".to_string());
            };
            attach_target = Some(validate_attach_target(value)?.to_owned());
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--attach=") {
            if attach_target.is_some() {
                return Err("--attach can only be specified once".to_string());
            }
            attach_target = Some(validate_attach_target(value)?.to_owned());
            index += 1;
            continue;
        }
        if arg == "--takeover" {
            takeover = true;
            index += 1;
            continue;
        }
        if arg == "--remote" {
            if remote_target.is_some() {
                return Err("--remote can only be specified once".to_string());
            }
            let Some(value) = args.get(index + 1) else {
                return Err("missing value for --remote".to_string());
            };
            remote_target = Some(validate_remote_target(value)?.to_owned());
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--remote=") {
            if remote_target.is_some() {
                return Err("--remote can only be specified once".to_string());
            }
            remote_target = Some(validate_remote_target(value)?.to_owned());
            index += 1;
            continue;
        }
        if arg == "--remote-keybindings" {
            if keybindings_seen {
                return Err("--remote-keybindings can only be specified once".to_string());
            }
            let Some(value) = args.get(index + 1) else {
                return Err("missing value for --remote-keybindings".to_string());
            };
            keybindings = RemoteKeybindings::parse(value)?;
            keybindings_seen = true;
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--remote-keybindings=") {
            if keybindings_seen {
                return Err("--remote-keybindings can only be specified once".to_string());
            }
            keybindings = RemoteKeybindings::parse(value)?;
            keybindings_seen = true;
            index += 1;
            continue;
        }

        cleaned.push(arg.clone());
        index += 1;
    }

    let attach_requested = attach_target.is_some();
    let remote = remote_target.map(|target| RemoteLaunch {
        target,
        keybindings,
        live_handoff,
        attach: attach_target.map(|target| RemoteAttach { target, takeover }),
    });
    if remote.is_none() && keybindings_seen {
        return Err("--remote-keybindings requires --remote".to_string());
    }
    if remote.is_none() && attach_requested {
        return Err("--attach requires --remote".to_string());
    }
    if remote.is_none() && takeover {
        return Err("--takeover requires --remote --attach".to_string());
    }
    if remote
        .as_ref()
        .is_some_and(|remote| remote.attach.is_none() && takeover)
    {
        return Err("--takeover requires --attach".to_string());
    }
    if remote.is_none() && live_handoff {
        cleaned.push("--handoff".to_string());
    }

    Ok((cleaned, remote))
}

pub(crate) fn validate_remote_target(target: &str) -> Result<&str, String> {
    if target.is_empty() {
        return Err("missing value for --remote".to_string());
    }
    if target.starts_with('-') {
        return Err("--remote target must not start with '-'".to_string());
    }
    Ok(target)
}

fn validate_attach_target(target: &str) -> Result<&str, String> {
    if target.is_empty() {
        return Err("missing value for --attach".to_string());
    }
    if target.starts_with('-') {
        return Err("--attach target must not start with '-'".to_string());
    }
    Ok(target)
}
