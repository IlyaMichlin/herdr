# herdr-direct build

This fork ([`IlyaMichlin/herdr`](https://github.com/IlyaMichlin/herdr)) publishes
its own `herdr` binaries as `herdr-direct-*` release assets, separate from the
upstream herdr.dev release channel. The binary is still `herdr`; only the
release asset names differ.

## What this fork adds

Everything in upstream Herdr works unchanged. The fork adds two features on top.

### 1. Direct remote attach: `--remote <host> --attach <target>`

Upstream `herdr --remote <host>` opens the full Herdr app UI for a remote
server. The fork adds `--attach`, which opens **one** remote agent, pane, or
terminal directly in your current terminal tab, with no Herdr UI around it:

```sh
herdr --remote cdagent --attach nma2-cc          # agent name
herdr --remote cdagent --attach w1:p1            # pane id
herdr --remote cdagent --attach nma2-cc --takeover  # replace another attached client
```

- The target is an agent name, pane ID, or terminal ID. The remote server
  resolves it, so you can use the names shown by `herdr agent list` on the host.
- Setup is the same as a normal `--remote` attach: platform detection, remote
  binary reuse or install, protocol check, and the managed SSH config.
- Keys go straight to the remote program; Herdr does not interpret them, and
  `--remote-keybindings` does not apply. **Detach with `ctrl+b q`.**
- Only one client can own a terminal at a time. A second attach fails unless
  you pass `--takeover`, which disconnects the first one.
- The remote terminal is resized to your local tab for as long as you are
  attached, and keeps that size after you detach if no one else is attached.
- The client runs locally, so a dropped SSH connection still restores your
  terminal (no stuck mouse-reporting mode), and local features such as image
  clipboard paste still reach the remote pane.

Typical setup: one terminal tab per remote agent, each running
`herdr --remote <host> --attach <agent>`.

### 2. Agent status in the cmux sidebar

[cmux](https://cmux.dev) shows agent status in its sidebar by injecting hooks
into a *local* `claude`. An agent that runs inside a remote Herdr server never
reaches those hooks, so the sidebar used to show nothing, or a stale
"Claude is waiting for your input" from an earlier session.

With this fork, a `--attach` client running in a cmux tab mirrors the attached
agent's state, as detected by the Herdr server, into that cmux workspace:

| Agent state | Sidebar pill | Notification |
| --- | --- | --- |
| Working | `Claude Code · Running` | Clears earlier notifications for the tab |
| Idle after working | `Claude Code · Idle` | "Finished" |
| Waiting for input or permission | `Claude Code · Needs input` | "Needs your input" |
| Agent exits, or you detach | Pill removed | — |

On attach, leftover notifications on that tab are cleared. This works for every
agent Herdr detects (Claude, Codex, and others), not just Claude.

- It turns on automatically when the client runs inside cmux (cmux sets
  `CMUX_SURFACE_ID`). There is nothing to configure.
- It uses its own sidebar key, `herdr_agent`, because cmux reserves
  `claude_code` for its own Claude wrapper.
- To turn it off, set `CMUX_CLAUDE_INTEGRATION_DISABLED=1`, the same switch
  that disables cmux's own Claude integration.
- **Both ends need this fork:** the local client (which calls the cmux CLI) and
  the remote Herdr server (which sends the agent status). An upstream server
  never sends it, so the sidebar stays empty. See below.

Troubleshooting: run the client with `HERDR_LOG=herdr::client=debug` and look
for `host_agent_status` lines in `~/.config/herdr/herdr-client.log`.

## Install

Install the fork on **both** your local machine and every remote host you
attach to.

**macOS (Apple Silicon) / Linux — one-liner:**

```sh
curl -fsSL https://github.com/IlyaMichlin/herdr/releases/latest/download/install-herdr-direct.sh | sh
```

This detects your OS/arch, downloads the matching `herdr-direct-<os>-<arch>`
asset from the latest release, and installs it as `herdr` in `~/.local/bin`.
On a remote host, `--remote` uses the `herdr` on the remote `PATH`, then
`~/.local/bin/herdr`, so make sure the fork's binary is the one that resolves
(`ssh <host> 'command -v herdr'`).

Environment overrides:

- `HERDR_INSTALL_DIR` — install location (default `~/.local/bin`).
- `HERDR_DIRECT_TAG` — pin a specific release, e.g. `herdr-direct-v0.7.5`.

**Manual:** download `herdr-direct-macos-aarch64` (or your platform) from the
[releases page](https://github.com/IlyaMichlin/herdr/releases), then:

```sh
chmod +x herdr-direct-macos-aarch64
mv herdr-direct-macos-aarch64 ~/.local/bin/herdr
```

To keep upstream Herdr installed side by side, install the fork under another
name, e.g. `~/.local/bin/herdr-direct`, and run `herdr-direct --remote ...`.

### Updating a remote host

The fork keeps upstream's version number (currently 0.9.5). Because of that:

- If a remote host has no Herdr at all, `--remote` installs the **upstream**
  0.9.5 binary from herdr.dev. That works for attaching but has no fork
  features, so install the fork there with the one-liner instead.
- A remote that already has any 0.9.5 binary counts as up to date, so
  `--remote` never replaces it. After a new fork release, update the remote
  explicitly, either with the one-liner on the host or from your machine:

  ```sh
  HERDR_REMOTE_BINARY=./herdr-direct-linux-aarch64 herdr --remote cdagent --attach nma2-cc
  ```

- Installing the binary does not restart a running remote server. To switch it
  to the new binary **without killing running agents**, run a live handoff on
  the host:

  ```sh
  ~/.local/bin/herdr server live-handoff --import-exe "$HOME/.local/bin/herdr" \
    --expected-protocol 22 --expected-version 0.9.5
  ```

  Attached clients are disconnected during the handoff; reattach afterwards.

Do not run `herdr update` on a fork install: it updates from herdr.dev and would
replace the fork with upstream Herdr once upstream ships a newer version.

## macOS notes

- Installing via the `curl | sh` one-liner does not trigger Gatekeeper
  quarantine. If you instead download the binary through a browser, macOS may
  block it as coming from an unidentified developer; clear it with:
  `xattr -d com.apple.quarantine ./herdr-direct-macos-aarch64`.
- The binary is unsigned (ad-hoc only) and not notarized — fine for people who
  trust this fork, not intended for wide public distribution.

## Cutting a release (maintainer of this fork)

Push a `herdr-direct-v*` tag or run the **Release herdr-direct** workflow
manually. CI builds all four platform binaries and attaches them plus
`install-herdr-direct.sh` to the GitHub release. Building locally needs Zig
0.16.0 exactly (`ZIG=/path/to/zig-0.16.0 cargo build --release`).
