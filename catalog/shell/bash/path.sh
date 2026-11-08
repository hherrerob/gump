# ~/.bashrc.d/path.sh (from gump): same PATH as the zsh config.

# macOS: Homebrew isn't on PATH by default; also its keg-only rustup (cargo, rustc).
for brew in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  [ -x "$brew" ] && eval "$("$brew" shellenv)" && break
done
[ -n "$HOMEBREW_PREFIX" ] && [ -d "$HOMEBREW_PREFIX/opt/rustup/bin" ] && PATH="$HOMEBREW_PREFIX/opt/rustup/bin:$PATH"

# User binaries first (Fedora's ~/.bashrc already does this; Arch's and macOS's don't).
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) PATH="$HOME/.local/bin:$HOME/bin:$PATH" ;; esac
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
export PATH
