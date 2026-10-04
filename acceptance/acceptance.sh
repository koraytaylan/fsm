#!/usr/bin/env bash
# Build the acceptance image and run the suite.
#
#   acceptance/acceptance.sh                 # every scenario
#   acceptance/acceptance.sh seal            # only scenarios whose name matches
#   RUST_VERSION=1.89.0 acceptance/acceptance.sh
#   FSM_EVIDENCE_DIR=/tmp/fsm-evidence acceptance/acceptance.sh
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
evidence="${FSM_EVIDENCE_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/fsm-acceptance.XXXXXXXX")}"
mkdir -p "$evidence"
evidence="$(cd "$evidence" && pwd)"
context_parent="$(mktemp -d "${TMPDIR:-/tmp}/fsm-acceptance-source.XXXXXXXX")"
trap 'rm -rf "$context_parent"' EXIT
PYTHONDONTWRITEBYTECODE=1 python3 "$here/suite/evidence.py" snapshot "$root" "$context_parent/source"

echo "building $image (rust $rust)…"
podman build \
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
    --name "fsm-acceptance-$$" \
    --volume "$evidence:/evidence:Z" \
    --env FSM_EVIDENCE_DIR=/evidence \
    "$image" "$@"
