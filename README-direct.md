# herdr-direct build

This fork ([`IlyaMichlin/herdr`](https://github.com/IlyaMichlin/herdr)) publishes
its own `herdr` binaries as `herdr-direct-*` release assets, separate from the
upstream herdr.dev release channel. The binary is still `herdr`; only the
release asset names differ.

## Install

**macOS (Apple Silicon) / Linux — one-liner:**

```sh
curl -fsSL https://github.com/IlyaMichlin/herdr/releases/latest/download/install-herdr-direct.sh | sh
```

This detects your OS/arch, downloads the matching `herdr-direct-<os>-<arch>`
asset from the latest release, and installs it as `herdr` in `~/.local/bin`.

Environment overrides:

- `HERDR_INSTALL_DIR` — install location (default `~/.local/bin`).
- `HERDR_DIRECT_TAG` — pin a specific release, e.g. `herdr-direct-v0.7.5`.

**Manual:** download `herdr-direct-macos-aarch64` (or your platform) from the
[releases page](https://github.com/IlyaMichlin/herdr/releases), then:

```sh
chmod +x herdr-direct-macos-aarch64
mv herdr-direct-macos-aarch64 ~/.local/bin/herdr
```

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
`install-herdr-direct.sh` to the GitHub release.
