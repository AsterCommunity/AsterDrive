#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
FRONTEND_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$FRONTEND_DIR/.." && pwd)
PORT="${ASTER_E2E_PORT:-3310}"
RUNTIME_DIR=$(mktemp -d "${TMPDIR:-/tmp}/asterdrive-e2e.XXXXXX")

cleanup() {
	rm -rf "$RUNTIME_DIR"
}

trap cleanup EXIT INT TERM

if [ ! -f "$FRONTEND_DIR/dist/index.html" ]; then
	echo "frontend-panel/dist is missing. Build the frontend before starting E2E." >&2
	exit 1
fi

ln -s "$FRONTEND_DIR/dist" "$RUNTIME_DIR/frontend-override"

printf '[e2e] runtime dir: %s\n' "$RUNTIME_DIR"
printf '[e2e] serving AsterDrive at http://127.0.0.1:%s\n' "$PORT"

export ASTER__SERVER__HOST=127.0.0.1
export ASTER__SERVER__PORT="$PORT"
export ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true
export ASTER__LOGGING__LEVEL=warn
export CARGO_TARGET_DIR="${ASTER_E2E_TARGET_DIR:-$REPO_ROOT/target/e2e}"
SERVER_BIN="$CARGO_TARGET_DIR/debug/aster_drive"

cd "$RUNTIME_DIR"
# A pinned executable avoids another worktree replacing target/debug/aster_drive mid-run.
if [ -n "${ASTER_E2E_SERVER_BIN:-}" ]; then
    if [ ! -x "$ASTER_E2E_SERVER_BIN" ]; then
        echo "ASTER_E2E_SERVER_BIN must name an executable file" >&2
        exit 1
    fi
    exec "$ASTER_E2E_SERVER_BIN"
fi
if [ "${ASTER_E2E_SKIP_BUILD:-}" = "1" ] && [ -x "$SERVER_BIN" ]; then
	exec "$SERVER_BIN"
fi

exec cargo run \
	--manifest-path "$REPO_ROOT/Cargo.toml" \
	--no-default-features \
	--features server
