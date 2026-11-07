# gump

My setup wizard for fresh Fedora, Arch and macOS installs. Pick apps in a TUI, gump installs
them with the native package manager and copies my configs. Safe to re-run: installed apps
are skipped, existing configs are kept.

```sh
curl -fsSL https://raw.githubusercontent.com/CHANGEME/gump/main/bootstrap.sh | sh
```

## Usage

```
gump                  # wizard (Space toggle, / search, Enter review, q quit)
gump list             # catalog for this OS
gump status           # what's installed
gump install zed git  # no wizard
```

Flags: `--verbose` (show the plan to approve and command output), `--dry-run` (implies --verbose), `--override` (replace differing configs, keeps a backup), `--agnostic`
(apps only, no configs), `--catalog PATH` (read catalog from disk).

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

Optional: `pre`/`post` commands, `[check]` (`command`, `path` or `shell`) and
`[extras.<os>]` for add-ons like VS Code extensions (`vscode = [...]`).

Pushing to `main` builds the binaries and updates the `latest` release that `bootstrap.sh`
downloads.
