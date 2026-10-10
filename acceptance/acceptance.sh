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
cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/fsm-acceptance"
mkdir -p "$cache_root"
task_directory="$(mktemp -d "$cache_root/run.XXXXXXXX")"
trap 'rm -rf "$task_directory"' EXIT
export TMPDIR="$task_directory/temporary"
mkdir -p "$TMPDIR"
evidence="${FSM_EVIDENCE_DIR:-$(mktemp -d "$cache_root/evidence.XXXXXXXX")}"
mkdir -p "$evidence"
evidence="$(cd "$evidence" && pwd)"
context_parent="$task_directory/context"
mkdir -p "$context_parent"
PYTHONDONTWRITEBYTECODE=1 python3 "$here/suite/evidence.py" snapshot "$root" "$context_parent/source"

echo "building $image (rust $rust)…"
podman build \
    --memory "$memory" --memory-swap "$memory" \
    --build-arg "RUST_VERSION=$rust" \
    -f "$context_parent/source/acceptance/Containerfile" \
    -t "$image" \
    "$context_parent/source"

echo
echo "evidence retained in $evidence"
# `--network=none` would be wrong: the HTTP transport scenario binds a port and
# connects to it, which needs a loopback interface. It stays inside the
# container's own namespace either way.
podman run --rm \
    --memory "$memory" --memory-swap "$memory" \
    --name "fsm-acceptance-$$" \
    --volume "$evidence:/evidence:Z" \
    --env FSM_EVIDENCE_DIR=/evidence \
    "$image" "$@"
