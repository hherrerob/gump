# Penpot launcher, run by ~/.local/bin/penpot. Penpot is a Docker Compose stack that only runs
# between `penpot start` and `penpot stop`; nothing starts it at boot.
set -e

dir="${XDG_CONFIG_HOME:-$HOME/.config}/penpot"
url="http://localhost:9001"
compose() { docker compose -f "$dir/docker-compose.yaml" "$@"; }

open_url() {
  if command -v xdg-open >/dev/null; then xdg-open "$url" >/dev/null 2>&1 &
  elif command -v open >/dev/null; then open "$url"
  fi
}

case "${1:-start}" in
  start)
    compose up -d
    echo "Penpot is starting at $url (the first start takes a minute). Stop it with: penpot stop"
    [ "$2" = "--no-open" ] || open_url
    ;;
  stop)   compose down ;;
  status) compose ps ;;
  logs)   shift; compose logs -f "${@:-penpot-backend}" ;;
  update)
    compose pull
    if [ -n "$(compose ps -q)" ]; then compose up -d; fi
    ;;
  open) open_url ;;
  *)
    cat <<EOF
Usage: penpot [command]
  start [--no-open]  Start Penpot and open $url (default)
  stop               Stop and remove the containers; your data stays in Docker volumes
  status             Show the containers
  logs [service]     Follow logs (default: penpot-backend)
  update             Pull newer images, restart if running
  open               Open $url
Mail sent by Penpot (sign-up, invites) shows up at http://localhost:8025
Config: $dir/docker-compose.yaml
EOF
    [ "$1" = "help" ] || [ "$1" = "-h" ] || [ "$1" = "--help" ] || exit 1
    ;;
esac
