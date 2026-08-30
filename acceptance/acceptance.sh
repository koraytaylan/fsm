#!/usr/bin/env bash
# Build the acceptance image and run the suite.
#
#   acceptance/acceptance.sh                 # every scenario
#   acceptance/acceptance.sh seal            # only scenarios whose name matches
#   RUST_VERSION=1.89.0 acceptance/acceptance.sh
#
# Everything runs inside the container: the binary under test is the one
# `cargo install --locked` produced from this tree, and the suite never touches
# the host's stores.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
image="${FSM_ACCEPTANCE_IMAGE:-fsm-acceptance}"
rust="${RUST_VERSION:-1.89.0}"

echo "building $image (rust $rust)…"
podman build \
    --build-arg "RUST_VERSION=$rust" \
    -f "$here/Containerfile" \
    -t "$image" \
    "$root"

echo
# `--network=none` would be wrong: the HTTP transport scenario binds a port and
# connects to it, which needs a loopback interface. It stays inside the
# container's own namespace either way.
exec podman run --rm \
    --name "fsm-acceptance-$$" \
    "$image" "$@"
