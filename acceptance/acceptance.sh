#!/usr/bin/env bash
# Build the acceptance image and run the suite.
#
#   acceptance/acceptance.sh                 # every scenario
#   acceptance/acceptance.sh seal            # only scenarios whose name matches
#   RUST_VERSION=1.89.0 acceptance/acceptance.sh
#   FSM_EVIDENCE_DIR="$HOME/.cache/fsm-evidence" acceptance/acceptance.sh
#   FSM_ACCEPTANCE_MEMORY=1g acceptance/acceptance.sh
#
# Requires host Python 3 and Podman. The source snapshot excludes Git metadata
# and credentials; reports, controlled-build receipts and diagnostics survive
# container removal. A dirty or filtered run remains ineligible for release.
#
# Everything runs inside the container: the binary under test is the one
# `cargo install --locked` produced from this tree, and the suite never touches
# the host's stores.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
image="${FSM_ACCEPTANCE_IMAGE:-fsm-acceptance}"
rust="${RUST_VERSION:-1.89.0}"
memory="${FSM_ACCEPTANCE_MEMORY:-1g}"
native="${FSM_ACCEPTANCE_DISPOSABLE_NATIVE:-0}"
if [[ "$native" == 1 ]] && [[ "${GITHUB_ACTIONS:-}" != true || "$(uname -s)" != Linux || "$EUID" == 0 ]]; then
    echo 'Native consumer execution requires an opted-in disposable Linux CI operator.' >&2
    exit 2
fi
cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/fsm-acceptance"
mkdir -p "$cache_root"
task_directory="$(mktemp -d "$cache_root/run.XXXXXXXX")"
export TMPDIR="$task_directory/temporary"
mkdir -p "$TMPDIR"
podman_command=(podman --tmpdir "$TMPDIR/podman")
if [[ "$native" == 1 ]]; then
    podman_command=(sudo -n env "TMPDIR=$TMPDIR" podman --tmpdir "$TMPDIR/podman")
fi
container_id=""
cleanup() {
    result=$?
    trap - EXIT
    if [[ -n "$container_id" ]]; then
        # Return task artifacts to the invoking CI operator; -hR preserves
        # links rather than following failed-store links outside this volume.
        timeout 20s sudo -n chown -hR --from=1000 "$EUID:$(id -g)" "$evidence" || result=1
        timeout 20s "${podman_command[@]}" logs "$container_id" > "$evidence/container.log" 2>&1 || true
        if ! timeout 20s "${podman_command[@]}" rm --force "$container_id" > "$evidence/container-removal.log" 2>&1; then
            result=1
        fi
        retired=false
        exists=0
        timeout 20s "${podman_command[@]}" container exists "$container_id" || exists=$?
        if [[ "$exists" == 1 ]]; then retired=true; else result=1; fi
        PYTHONDONTWRITEBYTECODE=1 python3 - "$evidence/container-retirement.json" "$container_id" "$retired" "$result" <<'PY'
import json,sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps(dict(container_id=sys.argv[2], removed=sys.argv[3]=='true', exit_code=int(sys.argv[4])), indent=2))
PY
    fi
    rm -rf "$task_directory"
    exit "$result"
}
trap cleanup EXIT
evidence="${FSM_EVIDENCE_DIR:-$(mktemp -d "$cache_root/evidence.XXXXXXXX")}"
mkdir -p "$evidence"
evidence="$(cd "$evidence" && pwd)"
context_parent="$task_directory/context"
mkdir -p "$context_parent"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/suite/evidence.py" snapshot "$root" "$context_parent/source"

echo "building $image (rust $rust)…"
if [[ "$(uname -s)" == Linux ]]; then free -h; swapon --show; fi
timeout 1200s "${podman_command[@]}" build \
    --memory "$memory" --memory-swap "$memory" \
    --build-arg "RUST_VERSION=$rust" \
    -f "$context_parent/source/acceptance/Containerfile" \
    -t "$image" \
    "$context_parent/source"

echo
echo "evidence retained in $evidence"
if [[ "$native" == 1 ]]; then
    candidate_container_id="$(timeout 60s "${podman_command[@]}" run --detach \
        --privileged --systemd=false --cgroupns=private --timeout=720 \
        --memory "$memory" --memory-swap "$memory" \
        --tmpfs /run:rw,size=64m,mode=755 --tmpfs /run/lock:rw,size=16m,mode=755 \
        --volume "$evidence:/evidence:Z" --user 0 \
        --entrypoint /bin/sh "$image" /src/acceptance/fixtures/container_init.sh)"
    [[ "$candidate_container_id" =~ ^[a-f0-9]{64}$ ]] || { echo 'Invalid owned container ID.' >&2; exit 1; }
    container_id="$candidate_container_id"
    timeout 30s "${podman_command[@]}" inspect "$container_id" > "$evidence/container-inspect.json"
    timeout 60s "${podman_command[@]}" exec --user 0 "$container_id" \
        /bin/sh -c 'until systemctl is-active --quiet system.slice; do sleep 0.1; done'
    timeout 30s "${podman_command[@]}" exec --user 0 "$container_id" \
        install -d -o fsm -g fsm -m 0755 /evidence
    timeout 660s "${podman_command[@]}" exec --user fsm \
        --env GITHUB_ACTIONS=true --env FSM_ACCEPTANCE_DISPOSABLE_NATIVE=1 \
        --env FSM_EVIDENCE_DIR=/evidence "$container_id" \
        python3 -m acceptance.suite.container "$@"
    exit 0
fi
# `--network=none` would be wrong: the HTTP transport scenario binds a port and
# connects to it, which needs a loopback interface. It stays inside the
# container's own namespace either way.
timeout 720s "${podman_command[@]}" run --rm \
    --memory "$memory" --memory-swap "$memory" \
    --name "fsm-acceptance-$$" \
    --volume "$evidence:/evidence:Z" \
    --env FSM_EVIDENCE_DIR=/evidence \
    "$image" "$@"
