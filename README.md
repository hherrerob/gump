# gump

> Cringe warning: This war vibed-sloped together in a couple of afternoons, it is incredibly useful for me, but might not be as useful to any other sane person.

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

Optional: `pre`/`post` commands, `[check]` (`command`, `path`, `shell`, or `login_shell` which must hold) and
`[extras.<os>]` for add-ons like VS Code extensions (`vscode = [...]`). A category with
`single = true` in `categories.toml` is pick-one (the Shell category).

Pushing to `main` builds the binaries and updates the `latest` release that `bootstrap.sh`
downloads.
