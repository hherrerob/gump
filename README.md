# gump

> "Gump! Why'd you put that together so quickly?" — "You told me to, Drill Sergeant."

My personal setup wizard for fresh **Fedora**, **Arch** and **macOS** installs. After a
clean install I run one command, tick the apps I want in a terminal UI, and gump installs
them with each OS's native package manager and drops in my config files.

It is safe to run any number of times. Apps that are already installed are skipped, and
existing config files are never touched unless I explicitly ask for it.

## On a fresh machine

```sh
curl -fsSL https://raw.githubusercontent.com/CHANGEME/gump/main/bootstrap.sh | sh
```

`bootstrap.sh` only needs `curl`. It detects the OS and CPU, downloads the matching prebuilt
binary to `~/.local/bin/gump` and starts the wizard. The whole catalog is compiled into the
binary, so nothing else is downloaded up front. (Replace `CHANGEME` here and in
`bootstrap.sh` with the GitHub user once the repo is pushed.)

```
curl … | sh
   │
   ▼
bootstrap.sh ── downloads gump for linux/macos × x86_64/arm64 ──▶ ~/.local/bin/gump
   │
   ▼
gump drill
   ├─ 1. Select   pick apps by category (badges: installed, n/a, settings state)
   ├─ 2. Review   exact commands and file writes; toggle override / agnostic / dry-run
   ├─ 3. sudo     password asked once, kept alive for the run
   └─ 4. Run      prerequisites → dependencies → apps → extras → config files
                  log: ~/.local/state/gump/last-run.log
```

## What it can install

| Category | Apps |
| --- | --- |
| Core | Git, CLI tools (ripgrep, fd, bat, fzf, jq, htop), fastfetch |
| Shell | zsh (also set as login shell), tmux |
| Terminal | Ghostty, kitty, Alacritty (not on macOS: Homebrew disabled its cask) |
| Editors | VS Code (+ config, + PlatformIO extension), Zed (+ config) |
| Browsers | Firefox, Brave, Google Chrome, Waterfox, Ungoogled Chromium |
| Languages | Rust (rustup, stable toolchain), Node.js + npm, uv, Go |
| AI harnesses | Claude Code, Codex, OpenCode, Orca (Stably) |
| Containers | Podman, Docker (OrbStack on macOS) |
| Making | Bambu Studio, OrcaSlicer, PlatformIO (pio CLI + serial access) |
| Fonts | JetBrains Mono Nerd Font, Fira Code, Inter |

Where things come from:

- **Fedora:** `dnf` (official repos plus vendor repos for VS Code, Brave, Chrome and Docker,
  and a COPR for Ghostty) and Flathub for Zed, Waterfox, Ungoogled Chromium, Bambu Studio
  and OrcaSlicer. Things nobody packages (Claude Code, Codex, OpenCode, Orca, PlatformIO,
  the Nerd Font) use a small install script.
- **Arch:** only `pacman` and the AUR through `yay`. Scripts and Flatpak are not allowed
  there; an app with no pacman/AUR package simply shows as n/a.
- **macOS:** Homebrew formulas and casks.

Every graphical app gets a normal launcher entry (app menu on Linux, `/Applications` on
macOS). Terminal tools like the AI harnesses, languages and CLI tools don't.

### Config files that come with it

| App | File | Contents |
| --- | --- | --- |
| Zed | `~/.config/zed/settings.json` | JetBrains Mono Nerd Font 14 (editor + terminal), theme follows the system, telemetry off |
| VS Code | `~/.config/Code/User/settings.json` (macOS: `~/Library/Application Support/Code/User/`) | Same font and size with ligatures, theme follows the system, telemetry off, no start page |
| VS Code | extension | `platformio.platformio-ide` |

Both editors depend on JetBrains Mono Nerd Font, so it is installed with them.

## Usage

```
gump                       # same as `gump drill`
gump drill                 # the interactive wizard
gump list                  # the catalog for this OS
gump status                # what's installed, and the state of each app's settings
gump install zed git       # install by app id without the wizard (--yes skips the prompt)
```

| Flag | Effect |
| --- | --- |
| `--dry-run` | Show every command and file write, run nothing |
| `--override` | Replace config files that differ from mine; the old one is kept as `<name>.gump-bak-<unix-time>` |
| `--agnostic` | Install apps only; never touch config files or extras |
| `--catalog PATH` | Read the catalog from a directory instead of the built-in one |

Wizard keys: `↑↓` move, `←→` switch pane, `Space` toggle the highlighted app, `c` clear the
selection, `/` search, `Enter` review, `q` quit. On the review screen `o`, `g` and `d` toggle
override, agnostic and dry-run.

`GUMP_OS=fedora|arch|macos` makes gump pretend to be another OS, which is handy with
`list` and `--dry-run`.

## How it works

**There is no state file.** Every run looks at the system itself, so things installed or
changed by hand are respected.

- **Is an app installed?** Its `[check]` passes (a command on PATH, a path that exists, or a
  shell test), *or* every package it lists for this OS is installed according to `rpm`,
  `pacman`, `flatpak`, `brew` or `code --list-extensions`. Installed apps skip all install
  steps. Note that this also means re-running gump never *updates* anything.
- **Config files:** missing → copied; identical → left alone; different → kept, and shown as
  "differs" (replaced, with a backup, only under `--override`). Config is applied even when
  the app was already installed, so a re-run fills in whatever is missing.
- **Extras** (VS Code extensions, add-on packages) behave like config: installed even when
  the app already is, skipped with `--agnostic`.

What gump does on its own when needed:

- Installs `yay` on Arch (from the AUR, via `base-devel` + `git`) before the first AUR app.
- Installs Homebrew on macOS with its official installer.
- Installs Flatpak and adds Flathub on Fedora.
- Installs an app's `depends` first (e.g. the Nerd Font before Ghostty, Zed and VS Code).
- Some apps change your account: zsh becomes the login shell, Docker adds you to the
  `docker` group, PlatformIO to `dialout` (Fedora) or `uucp` (Arch). Group changes apply
  after logging in again.

gump refuses to run as root (Homebrew does too, and `~` must be your home). It asks for the
sudo password once, keeps the session alive, and runs everything with `sudo -n`, so nothing
can hang on a hidden password prompt. One failed app doesn't stop the others.

## The catalog

```
catalog/
  categories.toml                  category ids, names and display order
  <category>/<app-id>.toml         one file per app
  <category>/<app-id>/<files>      only for apps with config files: the files themselves
```

### Adding an app

Create `catalog/<category>/<app-id>.toml`:

```toml
name = "Visual Studio Code"
description = "Code editor"
depends = ["jetbrains-mono-nerd"]        # optional; other app ids, installed first

[install.fedora]                         # no section for an OS → "n/a" there
pre = ["sudo rpm --import https://packages.microsoft.com/keys/microsoft.asc"]
dnf = ["code"]                           # also: flatpak = [...], script = '''...'''

[install.arch]
aur = ["visual-studio-code-bin"]         # also: pacman = [...] (nothing else on Arch)

[install.macos]
cask = ["visual-studio-code"]            # also: brew = [...], script = '''...'''

[check]                                  # optional; passes if ANY condition holds
command = "code"                         # on PATH
# path = "~/.local/bin/foo"              # exists
# shell = "rpm -q some-package"          # exits 0

[[settings]]
source = "settings.json"                 # file or directory in <category>/<app-id>/
target.linux = "~/.config/Code/User/settings.json"
target.macos = "~/Library/Application Support/Code/User/settings.json"
# or the same path everywhere:  target = "~/.config/foo/config"

[extras.fedora]                          # optional; packages that are part of the setup
vscode = ["platformio.platformio-ide"]   # VS Code extensions (extras only, any OS)
# dnf = ["some-plugin"]                  # same package keys as [install.*], no script
```

Rules:

- Steps run in order: `pre`, package lists, `script`, `post`. All of them are skipped when
  the app is already installed.
- A section with only a `script` must have a `[check]`, or gump couldn't tell it's installed.
- Targets support `~`, `$HOME` and `$XDG_*` variables (with the XDG defaults).
- Extras' `pre`/`post` run only when an extra is actually missing.
- The catalog is validated when gump starts: unknown keys, missing config files, unknown
  dependencies or a script on Arch are reported as a clear error (dependency cycles are
  caught when planning).

The catalog is embedded at build time. To try changes without rebuilding, use
`--catalog ./catalog`.

## Building and releasing

```sh
cargo build --release                      # target/release/gump
cargo run -- --catalog ./catalog drill --dry-run
```

Pushing to `main` runs `.github/workflows/release.yml`, which builds static Linux (x86_64,
aarch64) and macOS (arm64, x86_64) binaries and replaces the rolling `latest` GitHub
release. That release is what `bootstrap.sh` downloads, so editing the catalog and pushing
is all it takes for the next fresh install to get it.
