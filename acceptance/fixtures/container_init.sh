#!/bin/sh
# Only the explicitly opted-in disposable consumer VM starts this container.
set -eu
mount -o remount,rw /sys/fs/cgroup
exec /lib/systemd/systemd --unit=multi-user.target
