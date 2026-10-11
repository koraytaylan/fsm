# Disposable Linux/systemd host for the operational acceptance workflow.
# No repository credentials, runner registration, candidate binary or native
# authority enters the image; each container receives one ephemeral job.
FROM docker.io/library/rust:1.89.0-bookworm

ENV TMPDIR=/root/.cache/fsm-operational-runner/temporary \
    PYTHONDONTWRITEBYTECODE=1 \
    CARGO_BUILD_JOBS=1

RUN mkdir -p "$TMPDIR" \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       python3 git sudo systemd-sysv dbus mount gdb binutils curl ca-certificates \
       xz-utils build-essential unzip libicu72 libssl3 zlib1g \
    && useradd --create-home --uid 1000 --shell /bin/bash fsm \
    && printf 'fsm ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/fsm-operational-runner \
    && chmod 0440 /etc/sudoers.d/fsm-operational-runner \
    && install -d -o fsm -g fsm -m 0700 \
       /home/fsm/.cache /home/fsm/.cache/fsm-operational-runner \
       /home/fsm/.cache/fsm-operational-runner/temporary \
    && chown -R fsm:fsm /usr/local/rustup /usr/local/cargo \
    && rm -rf /var/lib/apt/lists/*

ENV TMPDIR=/home/fsm/.cache/fsm-operational-runner/temporary
# Let Podman's --systemd=always prepare the delegated cgroup mount; the
# rootful consumer's manual remount is refused in a rootless user namespace.
ENTRYPOINT ["/lib/systemd/systemd", "--unit=multi-user.target"]
