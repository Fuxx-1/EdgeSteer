#!/usr/bin/env bash
#
# Drive a running EdgeSteer agent from a script or an AI agent.
#
# The agent owns the DNS engine, system DNS, and the configuration file. The
# Flutter window is only a view, so everything a script needs is available over
# the agent's control protocol: this script is a thin JSON client for it.
#
# Usage:
#   scripts/edgesteer-ctl.sh status
#   scripts/edgesteer-ctl.sh ui <page>        # status|resolvers|ruleSets|preferred|json|system
#   scripts/edgesteer-ctl.sh engine start|stop|restart
#   scripts/edgesteer-ctl.sh system-dns enable|disable
#   scripts/edgesteer-ctl.sh autostart enable|disable
#   scripts/edgesteer-ctl.sh config read
#   scripts/edgesteer-ctl.sh config validate <file>
#   scripts/edgesteer-ctl.sh config save <file>
#   scripts/edgesteer-ctl.sh raw '<json command>'
#
# Output is JSON on stdout; diagnostics go to stderr. Exit codes:
#   0  the agent accepted the command
#   1  the agent rejected it (see .message in the output)
#   2  usage error
#   3  the agent is not running

set -euo pipefail

usage() {
  sed -n '3,25p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
}

state_dir() {
  case "$(uname -s)" in
    Darwin)
      printf '%s' "${HOME}/Library/Application Support/EdgeSteer"
      ;;
    *)
      printf '%s' "${XDG_CONFIG_HOME:-$HOME/.config}/edgesteer"
      ;;
  esac
}

record="$(state_dir)/agent.json"
if [[ ! -f "$record" ]]; then
  printf '{"ok":false,"message":"EdgeSteer agent is not running"}\n' >&2
  exit 3
fi

python3 - "$record" "$@" <<'PY'
import json
import socket
import sys

record_path = sys.argv[1]
arguments = sys.argv[2:]

with open(record_path, encoding="utf-8") as handle:
    record = json.load(handle)

host, _, port = record["address"].rpartition(":")


def command_from(arguments):
    if not arguments:
        raise SystemExit("missing command")
    head = arguments[0]
    rest = arguments[1:]

    simple = {
        "status": "status",
        "refresh": "refresh",
        "quit": "quit",
        "open-ui": "open_ui",
        "read-config": "read_config",
        "read-preferences": "read_preferences",
    }
    if head in simple:
        return simple[head]

    if head == "ui":
        page = rest[0] if rest else None
        return "open_ui" if page is None else {"open_ui": {"page": page}}

    if head == "engine":
        action = rest[0] if rest else ""
        return {
            "start": "start_engine",
            "stop": "stop_engine",
            "restart": "restart_engine",
        }[action]

    if head == "system-dns":
        return {
            "enable": "enable_system_dns",
            "disable": "disable_system_dns",
        }[rest[0]]

    if head == "autostart":
        return {
            "enable": "enable_auto_start",
            "disable": "disable_auto_start",
        }[rest[0]]

    if head == "legacy-service":
        return "remove_legacy_service"

    if head == "config":
        action = rest[0] if rest else ""
        if action == "read":
            return "read_config"
        path = rest[1] if len(rest) > 1 else ""
        if not path:
            raise SystemExit("config validate/save need a file path")
        with open(path, encoding="utf-8") as handle:
            contents = handle.read()
        if action == "validate":
            return {"validate_config": {"contents": contents}}
        if action == "save":
            return {"save_config": {"contents": contents}}
        raise SystemExit(f"unknown config action {action!r}")

    if head == "raw":
        return json.loads(rest[0])

    raise SystemExit(f"unknown command {head!r}")


try:
    command = command_from(arguments)
except SystemExit as error:
    print(f"edgesteer-ctl: {error}", file=sys.stderr)
    sys.exit(2)

request = {
    "protocol": record["protocol"],
    "token": record["token"],
    "command": command,
}

connection = socket.create_connection((host, int(port)), timeout=75)
try:
    connection.sendall(json.dumps(request).encode("utf-8") + b"\n")
    buffer = b""
    while not buffer.endswith(b"\n"):
        chunk = connection.recv(65536)
        if not chunk:
            break
        buffer += chunk
finally:
    connection.close()

if not buffer.strip():
    print('{"ok":false,"message":"agent closed the connection"}', file=sys.stderr)
    sys.exit(1)

response = json.loads(buffer)
print(json.dumps(response, indent=2, ensure_ascii=False))
sys.exit(0 if response.get("ok") else 1)
PY
