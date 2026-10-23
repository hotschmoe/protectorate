#!/usr/bin/env python3
"""Verify an installed node update, pointer confirmation, and rollback."""

import hashlib, importlib.util, json, os, pathlib, shutil, socket, subprocess, sys, tempfile, time, urllib.error, urllib.request

repo = pathlib.Path(__file__).resolve().parent.parent
if sys.platform != 'linux' or subprocess.run(('systemctl', '--user', 'show-environment'),
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
    print('skipping: systemd user manager is required')
    sys.exit(0)
arch = 'arm64' if os.uname().machine in ('aarch64', 'arm64') else 'amd64'
fixture = pathlib.Path(tempfile.mkdtemp(prefix='protectorate-installed-node-layout-'))
print('fixture', fixture, flush=True)
source = fixture/'source'; source.mkdir()
spec = importlib.util.spec_from_file_location('installed_hub_update', repo/'scripts/test-rust-installed-hub-update.py')
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
spec = importlib.util.spec_from_file_location('installed_node_update', repo/'scripts/test-rust-installed-node-update.py')
nodes = importlib.util.module_from_spec(spec); spec.loader.exec_module(nodes)
builds = fixture/'binaries'; builds.mkdir()
module.build_server('0.1.0', builds/'first')
module.build_server('0.1.1', builds/'second')
(source/'binaries').mkdir()
shutil.copy2(builds/'second', source/'binaries/second')
for version, name in (('0.1.0','first'),('0.1.1','second')):
    module.make_bundle(source/f'bundle-{version}',version,builds/name,arch)

def run(*args, **kw):
    return subprocess.run(args, check=True, text=True, **kw)

def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]

def wait(label, fn, seconds=90):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if fn(): return
        time.sleep(.25)
    raise AssertionError('timed out: '+label)

home = fixture / 'home'; home.mkdir()
mock = fixture / 'mock'; mock.mkdir()
(mock / 'systemctl').write_text('#!/bin/sh\nexit 0\n')
(mock / 'systemctl').chmod(0o755)
(fixture / 'no-delete.bash').write_text('rm() { :; }\n')
state = fixture / 'state'; releases = fixture / 'releases'
release_binary_dir = fixture/'node-candidates'; release_binary_dir.mkdir()
for asset in (source/'bundle-0.1.1').iterdir():
    if asset.is_file() and asset.name != 'index.html':
        shutil.copy2(asset, release_binary_dir/asset.name)
for component in module.COMPONENTS:
    os.link(release_binary_dir/f'{component}-linux-{arch}',release_binary_dir/component)
hub_bin = fixture/'hub-bin'; hub_bin.mkdir()
shutil.copy2(source/'bundle-0.1.1'/f'protectorate-rs-linux-{arch}', hub_bin/'protectorate-rs')
hub, hub_url = nodes.start_hub(fixture, hub_bin/'protectorate-rs', '--node-binary-dir', str(release_binary_dir))
owner_path = fixture/'owner-token'
node_port = port()
status, invite = nodes.hub_call(hub_url, 'NodeService/InviteNode', {'name': 'trial-peer', 'tier': 'care'})
assert status == 200, invite
code = fixture/'join.code'; code.write_text(invite['code']+'\n'); code.chmod(0o600)
env = dict(os.environ, HOME=str(home), BASH_ENV=str(fixture/'no-delete.bash'), TMPDIR=str(fixture), PATH=str(mock)+os.pathsep+os.environ['PATH'])
run('bash', str(repo/'install-node.sh'), '--bundle-dir', str(source/'bundle-0.1.0'), '--home', str(state), '--release-dir', str(releases),
    '--hub', invite['hubUrl'], '--name', 'trial-peer', '--code-file', str(code), '--ca-cert', str(fixture/'hub-state/ca/ca.pem'),
    '--listen', '127.0.0.1:%d' % node_port, '--no-start', cwd=repo, env=env)
assert os.readlink(releases/'current') == '0.1.0'
unit_path = home/'.config/systemd/user/protectorate-node.service'
assert 'current/protectorate-rs' in unit_path.read_text()
first_installed = releases/'0.1.0/protectorate-rs'
initial_hash = hashlib.sha256(first_installed.read_bytes()).hexdigest()
assert initial_hash == hashlib.sha256((source/f'bundle-0.1.0/protectorate-rs-linux-{arch}').read_bytes()).hexdigest()
candidate = release_binary_dir/f'protectorate-rs-linux-{arch}'
expected_hash = hashlib.sha256(candidate.read_bytes()).hexdigest()
node_unit = 'protectorate-installed-node-layout-%d.service' % os.getpid()
native_unit = 'protectorate-native-node-layout-%d.service' % os.getpid()
native_started = False
cli = str(repo/'target/debug/protectorate-cli')

def cli_run(*args):
    return subprocess.run((cli, '--hub', hub_url, '--token-file', str(owner_path), *args), capture_output=True, text=True, timeout=180)

def apply(name, body):
    status, reply = nodes.hub_call(hub_url, 'NodeService/ApplyUpdate', dict(body, name=name), timeout=90)
    assert status == 200, (status, reply)
    return reply

def agent_version():
    return nodes.metrics(hub_url, 'trial-peer')

try:
    run('/usr/bin/systemd-run','--user','--collect','--unit',node_unit,'--property=Type=exec','--property=KillMode=control-group','--property=Restart=no',str(releases/'current/protectorate-rs'),'node','serve','--home',str(state),'--file-root',str(fixture/'node-files'))
    wait('node 0.1.0', lambda: agent_version()=='0.1.0')
    guardian = run('/usr/bin/systemctl','--user','show',node_unit,'--property=MainPID','--value',capture_output=True).stdout.strip()
    update = cli_run('node','update','trial-peer','--json')
    lines=[json.loads(line) for line in update.stdout.splitlines() if line.strip()]
    assert lines[-1]['phase']=='UPDATE_PHASE_CONFIRMED', lines
    wait('node 0.1.1', lambda: agent_version()=='0.1.1')
    current = os.readlink(releases/'current')
    final_hash = hashlib.sha256((releases/current/'protectorate-rs').read_bytes()).hexdigest()
    old_hash = hashlib.sha256(first_installed.read_bytes()).hexdigest()
    manifest_hash = next(line.split()[0] for line in (releases/current/'SHA256SUMS').read_text().splitlines() if line.endswith(f'protectorate-rs-linux-{arch}'))
    after_guardian = run('/usr/bin/systemctl','--user','show',node_unit,'--property=MainPID','--value',capture_output=True).stdout.strip()
    evidence = {'phase':lines[-1]['phase'],'agentVersion':agent_version(),'pointer':current,'initialSha256':initial_hash,'oldReleaseSha256':old_hash,'expectedCandidateSha256':expected_hash,'installedSha256':final_hash,'manifestSha256':manifest_hash,'manifestMatches':final_hash==manifest_hash,'mainPidStable':guardian==after_guardian,'unit':node_unit}
    assert current == '0.1.1', evidence
    assert old_hash == initial_hash, evidence
    assert final_hash == expected_hash == manifest_hash, evidence
    assert guardian == after_guardian, evidence
    (fixture/'evidence.json').write_text(json.dumps(evidence,indent=2)+'\n')
    print(json.dumps(evidence,indent=2), flush=True)

    failed = fixture/'node-files/updates/failed-bundle'; failed.mkdir(parents=True)
    for component in module.COMPONENTS:
        asset = failed/f'{component}-linux-{arch}'
        if component == 'protectorate-rs':
            asset.write_text('#!/bin/sh\nif [ "$1" = version ] && [ "$2" = --json ]; then\n'
                f' printf \'{{"version":"0.1.2","os":"linux","arch":"{arch}"}}\\n\'\n'
                ' exit 0\nfi\nexit 42\n')
            asset.chmod(0o755)
        else:
            shutil.copy2(releases/current/f'{component}-linux-{arch}', asset)
    (failed/'VERSION').write_text('0.1.2\n')
    (failed/'release.json').write_text('{"version":"0.1.2"}\n')
    (failed/'SHA256SUMS').write_text(''.join(
        f'{hashlib.sha256((failed/f"{component}-linux-{arch}").read_bytes()).hexdigest()}  {component}-linux-{arch}\n'
        for component in module.COMPONENTS))
    failing_binary = failed/f'protectorate-rs-linux-{arch}'
    applied = apply('trial-peer', {'path':'updates/failed-bundle',
        'size':str(failing_binary.stat().st_size),
        'sha256':hashlib.sha256(failing_binary.read_bytes()).hexdigest(),
        'expectedVersion':'0.1.2'})
    assert applied['version'] == '0.1.2', applied
    marker = releases/'.node-update.json'
    wait('failed candidate rollback', lambda: json.loads(marker.read_text()).get('phase')=='rolledback')
    wait('node after rollback', lambda: agent_version()=='0.1.1')
    rollback = {'phase':json.loads(marker.read_text())['phase'],
        'pointer':os.readlink(releases/'current'),
        'nodeVersion':agent_version(),
        'oldReleaseSha256':hashlib.sha256(first_installed.read_bytes()).hexdigest(),
        'currentSha256':hashlib.sha256((releases/'current/protectorate-rs').read_bytes()).hexdigest(),
        'mainPidStable':guardian==run('/usr/bin/systemctl','--user','show',node_unit,
            '--property=MainPID','--value',capture_output=True).stdout.strip()}
    assert rollback == {'phase':'rolledback','pointer':'0.1.1','nodeVersion':'0.1.1',
        'oldReleaseSha256':initial_hash,'currentSha256':expected_hash,'mainPidStable':True}, rollback
    (fixture/'rollback-evidence.json').write_text(json.dumps(rollback,indent=2)+'\n')
    print(json.dumps(rollback,indent=2), flush=True)

    retry = fixture/'node-files/updates/failed-bundle-retry'; retry.mkdir(parents=True)
    for component in module.COMPONENTS:
        asset = retry/f'{component}-linux-{arch}'
        if component == 'protectorate-rs':
            asset.write_text('#!/bin/sh\nif [ "$1" = version ] && [ "$2" = --json ]; then\n'
                f' printf \'{{"version":"0.1.2","os":"linux","arch":"{arch}"}}\\n\'\n'
                ' exit 0\nfi\nexit 43\n')
            asset.chmod(0o755)
        else:
            shutil.copy2(releases/current/f'{component}-linux-{arch}', asset)
    (retry/'VERSION').write_text('0.1.2\n')
    (retry/'SHA256SUMS').write_text(''.join(
        f'{hashlib.sha256((retry/f"{component}-linux-{arch}").read_bytes()).hexdigest()}  {component}-linux-{arch}\n'
        for component in module.COMPONENTS))
    retry_binary = retry/f'protectorate-rs-linux-{arch}'
    apply('trial-peer', {'path':'updates/failed-bundle-retry',
        'size':str(retry_binary.stat().st_size),
        'sha256':hashlib.sha256(retry_binary.read_bytes()).hexdigest(),
        'expectedVersion':'0.1.2'})
    wait('retained-version retry rollback', lambda: json.loads(marker.read_text()).get('reason','').find('43') >= 0)
    retry_marker = json.loads(marker.read_text())
    assert retry_marker['phase']=='rolledback' and retry_marker['candidate_release']!='0.1.2'
    assert os.readlink(releases/'current')=='0.1.1' and agent_version()=='0.1.1'
    assert guardian==run('/usr/bin/systemctl','--user','show',node_unit,
        '--property=MainPID','--value',capture_output=True).stdout.strip()
    (fixture/'retry-evidence.json').write_text(json.dumps(retry_marker,indent=2)+'\n')
    print('retained-version retry after rollback passed', flush=True)

    native_root = fixture/'native/node-install'; native_root.mkdir(parents=True)
    native_first = native_root/'release-initial'; native_first.mkdir()
    shutil.copy2(first_installed, native_first/'protectorate-rs')
    shutil.copy2(releases/'0.1.0/protectorate-node-sandbox-helper',
        native_first/'protectorate-node-sandbox-helper')
    os.symlink('release-initial', native_root/'current')
    native_state = fixture/'native/state'; native_state.mkdir()
    native_files = fixture/'native/files'; native_files.mkdir()
    native_port = port()
    def native_version():
        return nodes.metrics(hub_url, 'trial-native')
    nodes.join(hub_url, fixture, native_root/'current/protectorate-rs', native_state, 'trial-native', native_port)
    run('/usr/bin/systemd-run','--user','--collect','--unit',native_unit,
        '--property=Type=exec','--property=KillMode=control-group','--property=Restart=no',
        str(native_root/'current/protectorate-rs'),'node','serve','--home',str(native_state),
        '--file-root',str(native_files))
    native_started = True
    wait('native initial node', lambda:native_version()=='0.1.0')
    native_guardian = run('/usr/bin/systemctl','--user','show',native_unit,
        '--property=MainPID','--value',capture_output=True).stdout.strip()
    native_update = cli_run('node','update','trial-native','--json')
    assert native_update.returncode == 0, (native_update.stdout,native_update.stderr)
    native_phases=[json.loads(line) for line in native_update.stdout.splitlines() if line.strip()]
    assert native_phases[-1]['phase']=='UPDATE_PHASE_CONFIRMED',native_phases
    wait('native updated node',lambda:native_version()=='0.1.1')
    native_pointer=os.readlink(native_root/'current')
    native_marker=json.loads((native_root/'.node-update.json').read_text())
    native_after=run('/usr/bin/systemctl','--user','show',native_unit,
        '--property=MainPID','--value',capture_output=True).stdout.strip()
    native_hash=hashlib.sha256((native_root/'current/protectorate-rs').read_bytes()).hexdigest()
    native_evidence={'phase':native_phases[-1]['phase'],'pointer':native_pointer,
        'marker':native_marker['phase'],'agentVersion':native_version(),
        'oldReleaseSha256':hashlib.sha256((native_first/'protectorate-rs').read_bytes()).hexdigest(),
        'newReleaseSha256':native_hash,'mainPidStable':native_guardian==native_after}
    assert native_pointer.startswith('release-0.1.1-') and native_marker['phase']=='confirmed'
    assert native_evidence['oldReleaseSha256']==initial_hash and native_hash==expected_hash
    assert native_guardian==native_after,native_evidence
    (fixture/'native-evidence.json').write_text(json.dumps(native_evidence,indent=2)+'\n')
    print(json.dumps(native_evidence,indent=2),flush=True)
finally:
    hub.terminate()
    try: hub.wait(timeout=10)
    except subprocess.TimeoutExpired:
        hub.kill(); hub.wait(timeout=10)
    subprocess.run(['/usr/bin/systemctl','--user','stop',node_unit],check=False)
    if native_started:
        subprocess.run(['/usr/bin/systemctl','--user','stop',native_unit],check=False)
