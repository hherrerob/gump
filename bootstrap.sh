#!/bin/sh
# Fresh-install bootstrap: downloads the gump binary for this machine and starts the wizard.
#   curl -fsSL https://raw.githubusercontent.com/<you>/gump/main/bootstrap.sh | sh
# Set GUMP_REPO=owner/repo to point at a fork. Only needs curl.
set -eu

REPO="${GUMP_REPO:-CHANGEME/gump}"
DEST="${GUMP_BIN_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) echo "gump: unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=$([ "$os" = macos ] && echo arm64 || echo aarch64) ;;
  *) echo "gump: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

url="https://github.com/$REPO/releases/download/latest/gump-$os-$arch"
mkdir -p "$DEST"
echo "Downloading $url"
curl -fSL --progress-bar -o "$DEST/gump.tmp" "$url"
chmod +x "$DEST/gump.tmp"
mv "$DEST/gump.tmp" "$DEST/gump"
echo "Installed $DEST/gump"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "Note: $DEST is not on your PATH yet; run it as $DEST/gump" ;;
esac

# Re-attach stdin to the terminal: under `curl | sh` it is the pipe, not the keyboard.
if [ -t 1 ] && [ -r /dev/tty ]; then
  exec "$DEST/gump" drill "$@" </dev/tty
fi
