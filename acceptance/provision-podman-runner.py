#!/usr/bin/env python3
"""Provision one real ephemeral Actions runner in the disposable Podman host.

Credentials are fetched by the authenticated gh client and sent only through
stdin into the private container; neither image nor state contains them.
The caller observes the job and calls stop; Podman also enforces a twelve-hour
deadline independently of the caller's lifetime.
"""

import argparse
import hashlib
import json
import os
import pwd
from pathlib import Path
import re
import subprocess
import time
import urllib.request


REPOSITORY = 'koraytaylan/fsm'
LABEL = 'fsm-operational-disposable'

# Rootless sysfs ancestors expose unmapped host-root ownership; retain every
# original kernel subtree beneath namespace-root-owned private mount ancestors.
SYSFS_VIEW = r'''
from pathlib import Path
import subprocess

def mount(*arguments):
    subprocess.run(['mount', *arguments], check=True, timeout=10)

cgroup = Path('/sys/fs/cgroup')
identity = (cgroup.stat().st_dev, cgroup.stat().st_ino)
original = Path('/run/fsm-operational-kernel-sysfs')
original.mkdir(mode=0o700)
mount('--rbind', '/sys', str(original))
mount('--make-rprivate', str(original))
mount('-t', 'tmpfs', '-o', 'size=1m,nosuid,nodev,mode=0755', 'tmpfs', '/sys')

def retain(source, destination):
    if source.is_symlink():
        destination.symlink_to(source.readlink())
    elif source.is_dir():
        destination.mkdir(mode=0o755)
        mount('--rbind', str(source), str(destination))
        mount('--make-rprivate', str(destination))
    else:
        raise RuntimeError('unexpected sysfs top-level entry')

for source in original.iterdir():
    if source.name == 'fs':
        destination = Path('/sys/fs')
        destination.mkdir(mode=0o755)
        for child in source.iterdir():
            retain(child, destination / child.name)
    else:
        retain(source, Path('/sys') / source.name)
assert identity == (cgroup.stat().st_dev, cgroup.stat().st_ino)
for path in (Path('/sys'), Path('/sys/fs'), cgroup):
    assert path.stat().st_uid == 0 and not path.stat().st_mode & 0o022
assert (cgroup / 'memory.max').read_text().strip() == '1073741824'
assert (cgroup / 'memory.swap.max').read_text().strip() == '0'
'''


def run(arguments, *, input=None, timeout=120):
    result = subprocess.run(arguments, input=input, text=True, capture_output=True,
                            timeout=timeout, check=False)
    if input is not None:
        diagnostic = result.stdout + result.stderr
        for secret in input.splitlines():
            if secret:
                diagnostic = diagnostic.replace(secret, '<redacted>')
        destination = Path(os.environ['TMPDIR']) / 'disposable-podman-host' / 'registration-diagnostic.log'
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, 'w') as stream:
            stream.write(diagnostic)
    if result.returncode:
        # Registration output and stdin must never become a public transcript.
        detail = '' if input is not None else ': ' + result.stderr[-1500:]
        raise RuntimeError(f'{arguments[0]} operation failed with exit {result.returncode}{detail}')
    return result.stdout


def api(path, method='GET'):
    return json.loads(run(['gh', 'api', '--method', method, path]) or 'null')


class Host:
    def __init__(self, rootful=False):
        temporary = Path(os.environ['TMPDIR']).resolve()
        if not temporary.is_relative_to(Path.home() / '.cache'):
            raise ValueError('TMPDIR must be an owned home task cache')
        self.rootful = rootful
        self.cache = temporary / ('disposable-podman-rootful' if rootful else 'disposable-podman-host')
        self.state = self.cache / 'runner-state.json'
        for name in ('runtime', 'storage', 'run', 'temporary'):
            (self.cache / name).mkdir(parents=True, exist_ok=True, mode=0o700)
        os.environ['XDG_RUNTIME_DIR'] = str(self.cache / 'runtime')
        self.podman = ['podman', '--root', str(self.cache / 'storage'),
                       '--runroot', str(self.cache / 'run'), '--tmpdir',
                       str(self.cache / 'temporary'), '--storage-driver', 'vfs']
        if rootful:
            self.podman = ['sudo', '-n', 'env', 'TMPDIR=' + str(temporary),
                           'PYTHONDONTWRITEBYTECODE=1'] + self.podman
        info = json.loads(run(self.podman + ['info', '--format', 'json']))
        if info['host']['security']['rootless'] == rootful:
            raise ValueError('Podman runtime does not match the selected provisioning mode')

    def isolation(self):
        if not self.rootful:
            return ['--privileged']
        name = pwd.getpwuid(os.getuid()).pw_name
        if os.getuid() == 0 or os.getgid() == 0:
            raise ValueError('the provisioner requires a non-root caller')
        options = ['--cap-add=SYS_ADMIN', '--cap-add=MKNOD', '--cap-add=SYS_PTRACE',
                   '--security-opt=apparmor=unconfined']
        for kind in ('uid', 'gid'):
            rows = [line.split(':') for line in Path('/etc/sub' + kind).read_text().splitlines()
                    if line.split(':', 1)[0] == name]
            first = next((int(row[1]) for row in rows
                          if int(row[1]) >= 65536 and int(row[2]) >= 65536), None)
            if first is None:
                raise ValueError('the caller needs a complete assigned subordinate ID range')
            caller = os.getuid() if kind == 'uid' else os.getgid()
            options.extend(['--' + kind + 'map=0:' + str(caller) + ':1',
                            '--' + kind + 'map=1:' + str(first) + ':65535'])
        return options

    def prepare_random_device(self):
        if not self.rootful:
            return []
        device = self.cache / 'kernel-random-source'
        run(['sudo', '-n', 'env', 'TMPDIR=' + os.environ['TMPDIR'],
             'python3', '-B', '-c',
             "import os,stat,sys; from pathlib import Path; "
             "path=Path(sys.argv[1]); uid=int(sys.argv[2]); gid=int(sys.argv[3]); "
             "assert uid!=0 and gid!=0; "
             "exists=path.exists(); "
             "os.mknod(path,stat.S_IFCHR|0o600,os.makedev(1,8)) if not exists else None; "
             "os.chown(path,uid,gid) if not exists else None; "
             "metadata=path.lstat(); "
             "assert stat.S_ISCHR(metadata.st_mode) and metadata.st_rdev==os.makedev(1,8) "
             "and metadata.st_uid==uid and metadata.st_gid==gid "
             "and stat.S_IMODE(metadata.st_mode)==0o600",
             str(device), str(os.getuid()), str(os.getgid())])
        return ['--mount=type=bind,source=' + str(device)
                + ',destination=/run/fsm-kernel-random,ro']

    def save(self, state):
        descriptor = os.open(self.state, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, 'w') as stream:
            json.dump(state, stream, indent=2)
            stream.write('\n')

    def exec(self, container, command, *, user='0', input=None):
        interactive = ['--interactive'] if input is not None else []
        return run(self.podman + ['exec'] + interactive + ['--user', user, '--env', 'HOME=/home/fsm',
                   container] + command, input=input)

    def start(self):
        if self.state.exists():
            raise ValueError('inspect and stop the recorded runner before starting another')
        release = api('repos/actions/runner/releases/latest')
        asset = next(item for item in release['assets']
                     if re.fullmatch(r'actions-runner-linux-x64-[0-9.]+\.tar\.gz', item['name']))
        digest = asset.get('digest', '')
        if not re.fullmatch(r'sha256:[a-f0-9]{64}', digest):
            raise ValueError('official runner asset has no SHA-256 digest')
        archive = self.cache / asset['name']
        if not archive.exists():
            partial = archive.with_suffix('.partial')
            request = urllib.request.Request(asset['browser_download_url'],
                                             headers={'User-Agent': 'fsm-operational-acceptance'})
            deadline = time.monotonic() + 300
            with urllib.request.urlopen(request, timeout=30) as source, partial.open('wb') as target:
                copied = 0
                while block := source.read(1024 * 1024):
                    copied += len(block)
                    if copied > asset['size'] or time.monotonic() > deadline:
                        raise ValueError('runner download exceeded its declared size or deadline')
                    target.write(block)
            partial.rename(archive)
        with archive.open('rb') as stream:
            archive_digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if archive.stat().st_size != asset['size'] or archive_digest != digest[7:]:
            raise ValueError('runner archive differs from the official release asset')
        image = json.loads(run(self.podman + ['image', 'inspect',
                              'localhost/fsm-operational-disposable:local']))[0]['Id']
        name = 'fsm-operational-' + str(time.time_ns())
        container = run(self.podman + ['create', '--name', name] + self.isolation()
                        + self.prepare_random_device() + [
            '--systemd=always', '--cgroupns=private', '--pid=private',
            '--network=slirp4netns',
            '--timeout=43200', '--memory=1g', '--memory-swap=1g', '--user=0', image],
            timeout=300).strip()
        if not re.fullmatch(r'[a-f0-9]{64}', container):
            raise ValueError('Podman did not return a container identity')
        state = dict(container_id=container, image_id=image, runner_name=name,
                     runner_asset=asset['name'], runner_asset_sha256=digest[7:],
                     runner_id=None, started_at=time.time(), deadline_seconds=43200)
        self.save(state)
        try:
            run(self.podman + ['start', container])
            self.exec(container, ['bash', '-ec',
                'timeout 45 bash -c \'until systemctl is-active --quiet multi-user.target; do sleep 1; done\'; '
                'test "$(cat /sys/fs/cgroup/memory.max)" = 1073741824; '
                'test "$(cat /sys/fs/cgroup/memory.swap.max)" = 0; '
                'mkdir -p /home/fsm/actions-runner'])
            self.exec(container, ['bash', '-ec',
                'chown fsm:fsm /home/fsm; '
                'chown -R fsm:fsm /home/fsm/.cache /usr/local/rustup /usr/local/cargo'])
            self.exec(container, ['python3', '-B', '-c', SYSFS_VIEW])
            if self.rootful:
                self.exec(container, ['bash', '-ec',
                    'mount --bind /run/fsm-kernel-random /dev/random; '
                    'mount -o remount,bind,ro /dev/random'])
            self.exec(container, ['python3', '-B', '-c',
                "import os,stat; from pathlib import Path; "
                "metadata=Path('/dev/random').stat(); "
                "assert stat.S_ISCHR(metadata.st_mode) and metadata.st_uid==0 "
                "and metadata.st_rdev==os.makedev(1,8)"])
            self.exec(container, ['bash', '-ec',
                'for tool in python3 rustup cargo git sudo systemd-run gdb nm free swapon; '
                'do command -v "$tool" >/dev/null; done; '
                'for directory in /home/fsm /home/fsm/.cache /usr/local/rustup /usr/local/cargo; '
                'do test -w "$directory"; done; '
                'test -w /usr/local/rustup/settings.toml'], user='fsm')
            run(self.podman + ['cp', str(archive), container + ':/home/fsm/runner.tar.gz'])
            self.exec(container, ['bash', '-ec',
                'tar -xzf /home/fsm/runner.tar.gz -C /home/fsm/actions-runner; '
                'rm /home/fsm/runner.tar.gz; chown -R fsm:fsm /home/fsm/actions-runner'])
            token = api(f'repos/{REPOSITORY}/actions/runners/registration-token', 'POST')['token']
            self.exec(container, ['bash', '-ec',
                'read -r registration_token; cd /home/fsm/actions-runner; '
                'exec ./config.sh --unattended --ephemeral --disableupdate '
                '--url https://github.com/koraytaylan/fsm --labels fsm-operational-disposable '
                '--name "$1" --token "$registration_token"', 'registration', name],
                user='fsm', input=token + '\n')
            token = None
            registration = json.loads(self.exec(container,
                ['cat', '/home/fsm/actions-runner/.runner']).lstrip('\ufeff'))
            state['runner_id'] = registration['agentId']
            self.save(state)
            self.exec(container, ['systemd-run', '--unit=fsm-actions-runner',
                '--property=User=fsm', '--property=WorkingDirectory=/home/fsm/actions-runner',
                '--setenv=HOME=/home/fsm',
                '--setenv=PATH=/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
                '--setenv=RUSTUP_HOME=/usr/local/rustup', '--setenv=CARGO_HOME=/usr/local/cargo',
                '--setenv=TMPDIR=/home/fsm/.cache/fsm-operational-runner/temporary',
                '--setenv=CARGO_BUILD_JOBS=1', '--setenv=PYTHONDONTWRITEBYTECODE=1',
                '/home/fsm/actions-runner/run.sh'])
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                runner = api(f'repos/{REPOSITORY}/actions/runners/{state["runner_id"]}')
                if runner['status'] == 'online':
                    run(['gh', 'variable', 'set', 'FSM_OPERATIONAL_RUNNER_READY',
                         '--repo', REPOSITORY, '--body', 'true'])
                    print(json.dumps({**state, 'status': 'online', 'busy': runner['busy']}))
                    return
                time.sleep(2)
            raise RuntimeError('ephemeral runner did not become online before its deadline')
        except BaseException:
            self.stop()
            raise

    def status(self):
        state = json.loads(self.state.read_text())
        inspection = json.loads(run(self.podman + ['inspect', state['container_id']]))[0]
        print(json.dumps({**state, 'container_state': inspection['State'],
                          'registered_runners': api(f'repos/{REPOSITORY}/actions/runners')['runners']}))

    def stop(self):
        state = json.loads(self.state.read_text())
        container = state['container_id']
        failures = []
        try:
            run(['gh', 'variable', 'set', 'FSM_OPERATIONAL_RUNNER_READY',
                 '--repo', REPOSITORY, '--body', 'false'])
        except Exception as error:
            failures.append(str(error))
        try:
            exists = subprocess.run(self.podman + ['container', 'exists', container],
                                    capture_output=True, timeout=30, check=False)
            if exists.returncode not in (0, 1):
                raise RuntimeError('cannot determine owned container existence')
            if exists.returncode == 0:
                (self.cache / (state['runner_name'] + '.log')).write_text(run(self.podman + ['logs', container]))
                inspection = run(self.podman + ['inspect', container])
                (self.cache / (state['runner_name'] + '-inspect.json')).write_text(inspection)
        except Exception as error:
            failures.append(str(error))
        # Remote API or diagnostic failures must not strand a local container.
        run(self.podman + ['rm', '--force', '--ignore', container])
        try:
            for runner in api(f'repos/{REPOSITORY}/actions/runners')['runners']:
                if runner['name'] == state['runner_name']:
                    api(f'repos/{REPOSITORY}/actions/runners/{runner["id"]}', 'DELETE')
        except Exception as error:
            failures.append(str(error))
        if failures:
            # Keep the original identity available for a remote cleanup retry.
            raise RuntimeError('local container removed; ' + '; '.join(failures))
        self.state.rename(self.cache / (state['runner_name'] + '-retired.json'))
        print(json.dumps({'container_id': container, 'removed': True}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('start', 'status', 'stop'))
    parser.add_argument('--rootful', action='store_true',
                        help='rootful runtime with container root mapped to the non-root caller')
    arguments = parser.parse_args()
    getattr(Host(rootful=arguments.rootful), arguments.action)()
