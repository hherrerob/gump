# gump

> ⚠️ Cringe warning: This was vibed-sloped together in a couple of afternoons, it is incredibly useful for me, but might not be as useful to any other sane person.

My setup wizard for fresh Fedora, Arch and macOS installs. Pick apps in a TUI, gump installs
them with the native package manager and copies my configs. Safe to re-run: installed apps
are skipped, existing configs are kept.

```sh
curl -fsSL https://raw.githubusercontent.com/hherrerob/gump/main/bootstrap.sh | sh
```

## Usage

```
gump                  # wizard (Space toggle, / search, Enter install, q quit)
gump list             # catalog for this OS
gump status           # what's installed
gump install zed git  # no wizard
```

| Flag | What it does |
|---|---|
| `-v`, `--verbose` | Show the plan to approve and the command output |
| `--dry-run` | Show what would happen without changing anything (implies `--verbose`) |
| `--override` | Replace configs that differ (the old file is backed up) |
| `--agnostic` | Install apps only, never touch configs |
| `--catalog PATH` | Read the catalog from disk instead of the built-in one |

## Catalog

Options included so far:

<details>
<summary><b>Core</b></summary>

- CLI tools: ripgrep, fd, bat, fzf, jq, htop
- fastfetch: System info at a glance

</details>

<details>
<summary><b>Shell</b> (pick one)</summary>

- bash: Back to bash as your login shell, with the same prompt as zsh
- zsh: Z shell with autosuggestions and syntax highlighting; becomes your login shell

</details>

<details>
<summary><b>Terminal</b></summary>

- Alacritty: Minimal GPU-accelerated terminal emulator
- Ghostty: GPU-accelerated terminal emulator
- kitty: GPU-accelerated terminal emulator
- tmux: Terminal multiplexer

</details>

<details>
<summary><b>Editors</b></summary>

- Visual Studio Code: Code editor
- Zed: Fast collaborative code editor

</details>

<details>
<summary><b>Git &amp; forges</b></summary>

- Forgejo CLI (cli): fj: Forgejo repos, PRs and issues (Codeberg and self-hosted)
- Git (cli): Version control
- Gitea CLI (cli): tea: Gitea repos, PRs and issues (also works with Forgejo)
- GitHub CLI (cli): gh: GitHub repos, PRs, issues and Actions
- GitLab CLI (cli): glab: GitLab merge requests, issues and pipelines

</details>

<details>
<summary><b>Browsers</b></summary>

- Brave: Chromium-based browser with built-in ad blocking
- Firefox: Web browser
- Google Chrome: Google's browser
- Ungoogled Chromium: Chromium without Google services
- Waterfox: Privacy-focused Firefox fork

</details>

<details>
<summary><b>Languages</b></summary>

- Go: Go toolchain
- Node.js: JavaScript runtime with npm
- Rust: rustup with the stable toolchain
- uv: Python package and project manager

</details>

<details>
<summary><b>AI harnesses</b></summary>

- Claude Code (cli): Anthropic's coding agent in the terminal
- Codex (cli): OpenAI's coding agent in the terminal
- herdr (cli): Terminal multiplexer for coding agents
- OpenCode (cli): Open-source coding agent for any model provider
- Orca: Desktop workspace for running coding agents in parallel (Stably)

</details>

<details>
<summary><b>Containers</b></summary>

- Docker: Docker Engine (OrbStack on macOS)
- Podman: Daemonless containers

</details>

<details>
<summary><b>Making</b></summary>

- Backplane: Agentic CAD, a KiCad fork (alpha)
- Bambu Studio: Slicer for Bambu Lab printers
- esptool: Flash and erase ESP32/ESP8266 chips from the terminal
- KiCad: Schematic and PCB design
- OrcaSlicer: Open-source slicer for most 3D printers
- PlatformIO: Embedded toolchain for ESP32 and friends (pio CLI + VS Code extensions)

</details>

<details>
<summary><b>Fonts</b></summary>

- Fira Code: Monospace font with ligatures
- Inter: UI sans-serif font
- JetBrains Mono Nerd Font: Monospace font patched with Nerd Font icons

</details>

# How to add stuff to the catalog:

One file per app in `catalog/<category>/<app>.toml`; config files go in
`catalog/<category>/<app>/`.

```toml
name = "Zed"
description = "Code editor"
depends = ["jetbrains-mono-nerd"]

[install.fedora]
flatpak = ["dev.zed.Zed"]        # dnf, flatpak, script

[install.arch]
pacman = ["zed"]                 # pacman, aur (yay) only

[install.macos]
cask = ["zed"]                   # brew, cask, script

[[settings]]
source = "settings.json"
target = "~/.config/zed/settings.json"
```

Optional keys:

| Key | What it does |
|---|---|
| `pre` / `post` | Commands to run before / after installing |
| `cli = true` | Terminal-only app with no desktop launcher; the wizard shows it as `(cli)` |
| `[check]` | How to tell the app is already set up: `command`, `path`, `shell` or `login_shell` (which must hold) |
| `[extras.<os>]` | Add-ons like VS Code extensions (`vscode = [...]`) |

A category with `single = true` in `categories.toml` is pick-one (the Shell category).

Pushing to `main` builds the binaries and updates the `latest` release that `bootstrap.sh`
downloads.
