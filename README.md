# gump

> "Gump! Why did you put that together so quickly?" — "You told me to, Drill Sergeant."

A Ninite-style setup wizard for fresh **Fedora**, **Arch** and **macOS** installs. Pick apps by
category in a TUI; gump installs them with the native package manager and drops in your
settings files. It is safe to run any number of times: installed apps are skipped and existing
settings files are never touched unless you ask.

## Fresh machine

```sh
curl -fsSL https://raw.githubusercontent.com/CHANGEME/gump/main/bootstrap.sh | sh
```

This downloads the right binary to `~/.local/bin/gump` and starts the wizard. The catalog is
built into the binary, so nothing else is needed. (Replace `CHANGEME` here and in
`bootstrap.sh` with your GitHub user once the repo is pushed.)

## Usage

```
gump                      # same as `gump drill`
gump drill                # interactive wizard
gump list                 # catalog for this OS (GUMP_OS=arch|fedora|macos previews another)
gump status               # installed? settings missing / differs / current?
gump install vscode git   # non-interactive; also accepts profile names
gump install essentials --yes
```

| Flag | Effect |
| --- | --- |
| `--override` | Replace settings files that differ; the old file is kept as `<name>.gump-bak-<unix-time>` |
| `--agnostic` | Install apps only, never touch settings files or extras |
| `--dry-run` | Show every command and file write without doing anything |
| `--catalog PATH` | Use a catalog directory on disk instead of the embedded one |

In the wizard: `Space` toggle, `a` toggle whole category, `/` search, `p` profiles, `Enter`
review. On the review screen `o` / `g` / `d` toggle override / agnostic / dry-run.

### How idempotency works

There is no state file. Every run asks the system:

- **Installed?** The app's `[check]` passes, *or* every package it lists for this OS is
  installed (`rpm`, `pacman`, `flatpak`, `brew list`). Installed apps skip all install steps.
- **Settings:** missing → copied; identical → nothing; different → kept (replaced with backup
  under `--override`). Settings are applied even when the app was already installed, so
  re-running fills in anything missing.

sudo is requested once before installing; every child process runs with `sudo -n`, so nothing
can stall waiting for a password. Run gump as your normal user, not root.

## Catalog

```
catalog/
  categories.toml                   # category ids + display order
  profiles.toml                     # presets: name = ["app", ...]
  <category>/<app-id>/app.toml      # one directory per app
  <category>/<app-id>/<settings>    # files (or directories) the manifest references
```

### Adding an app

Create `catalog/<category>/<app-id>/app.toml`:

```toml
name = "Visual Studio Code"
description = "Code editor"
depends = ["jetbrains-mono-nerd"]        # optional, installed first

[install.fedora]                         # omit an OS section → "n/a" there
pre = ["sudo rpm --import https://packages.microsoft.com/keys/microsoft.asc"]
dnf = ["code"]                           # also: flatpak = [...]

[install.arch]
aur = ["visual-studio-code-bin"]         # also: pacman = [...], flatpak = [...]

[install.macos]
cask = ["visual-studio-code"]            # also: brew = [...]

[check]                                  # optional; passes if ANY condition holds
command = "code"                         # on PATH
# path = "~/.local/share/fonts/Foo"      # exists
# shell = "rpm -q some-package"          # exits 0

[[settings]]
source = "settings.json"                 # file or directory next to app.toml
target.linux = "~/.config/Code/User/settings.json"
target.macos = "~/Library/Application Support/Code/User/settings.json"
# or, same path everywhere:  target = "~/.config/foo/config"

[extras.fedora]                          # optional; packages that are part of the setup
dnf = ["some-plugin"]                    # same keys as [install.*] except script
```

Steps run in order: `pre`, package lists, `script`, `post`. A section with only a `script`
must have a `[check]`, or gump couldn't tell it's already installed. Targets support `~`,
`$HOME` and `$XDG_CONFIG_HOME`-style variables (with XDG defaults).

`[extras.<os>]` lists add-on packages that belong to the app's configuration, like zsh
plugins (see `catalog/shell/zsh`). They count as settings, not as separate apps: they are
installed even when the app already is, only the missing ones are installed, `--agnostic`
skips them, and a missing extra shows the app's settings as "missing". Their `pre`/`post`
run only when something is missing.

When needed, gump installs `paru` (AUR), Homebrew, and Flatpak + Flathub itself.

## Development

```sh
cargo test                                   # includes validating the embedded catalog
cargo run -- list
cargo run -- drill --dry-run
cargo run -- --catalog ./catalog status      # read catalog from disk without rebuilding
```

Pushing to `main` runs `.github/workflows/release.yml`: tests, builds static Linux (x86_64,
aarch64) and macOS (arm64, x86_64) binaries, and replaces the rolling `latest` release that
`bootstrap.sh` downloads.
