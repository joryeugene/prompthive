#!/usr/bin/env python3
"""Test ph update against a fake Cargo and temporary installs, never the user's installation."""
import os, sys
if os.name != 'posix':
    print('EXCEPTION: updater PTY checks require POSIX; native Windows uses the manual fallback.')
    raise SystemExit(0)
import json, pathlib, pty, shutil, signal, subprocess, tempfile, threading, time

binary = pathlib.Path(sys.argv[1]).resolve()
registry = 'registry+https://github.com/rust-lang/crates.io-index'
current = subprocess.check_output([str(binary), '--version'], text=True).strip().split(' ', 1)[1]
next_version = '99.0.1'
def meta(version=None, source=registry):
    version = current if version is None else version
    return {'installs': {f'prompthive {version} ({source})': {'bins': ['ph'], 'features': ['tui'], 'all_features': False, 'no_default_features': True, 'profile': 'release', 'target': 'x86_64-unknown-linux-gnu'}}}

FAKE = r'''#!/usr/bin/python3
import json, os, pathlib, shutil, sys
args=sys.argv[1:]
with open(os.environ['FAKE_LOG'],'a') as f: f.write(json.dumps(args)+'\n')
root=pathlib.Path(args[args.index('--root')+1]); stage=root.resolve() != pathlib.Path(os.environ['CARGO_HOME']).resolve(); initial=stage and '--version' in args
mode=os.environ['FAKE_MODE']
if mode=='advance' and stage and not initial:
    pathlib.Path(os.environ['FAKE_REGISTRY']).write_text('99.0.2')
if mode=='advance' and not stage and '--version' not in args:
    sys.exit(88)  # latest advanced to 99.0.2; only pinning preserves the staged selection
if mode=='race' and not stage:
    pathlib.Path(os.environ['FAKE_READY']).write_text('ready')
    import time
    while not pathlib.Path(os.environ['FAKE_GATE']).exists(): time.sleep(.01)
    sys.exit(7)
if mode=='signal' and not stage:
    replacement=root/'bin/partial-ph'
    replacement.write_bytes(b'partial installation')
    replacement.replace(root/'bin/ph')
    (root/'.crates2.json').write_text(os.environ['FAKE_NEW_META'])
    pathlib.Path(os.environ['FAKE_READY']).write_text('ready')
    import time; time.sleep(60)
if mode=='failure': sys.exit(7)
if mode=='default_failure' and initial and '--target' not in args: sys.exit(7)
if mode=='default_failure_before' and not stage: sys.exit(7)
if initial:
    (root/'bin').mkdir(parents=True,exist_ok=True)
    if mode=='mismatch' or (mode.startswith('default_') and '--target' in args): (root/'bin/ph').write_bytes(b'unrelated')
    else: shutil.copy2(os.environ['FAKE_ORIGINAL'],root/'bin/ph')
    data=json.loads(os.environ['FAKE_NEW_META'] if mode=='explicit_wrong_version' else os.environ['FAKE_META'])
    if mode=='default_wrong_target' and '--target' not in args:
        next(iter(data['installs'].values()))['target']='aarch64-unknown-linux-gnu'
    if mode=='explicit_wrong_target':
        next(iter(data['installs'].values()))['target']='aarch64-unknown-linux-gnu'
    (root/'.crates2.json').write_text(json.dumps(data))
else:
    if mode in ('updated','declined','false_success','advance','wrong_bytes','failure_after','race','signal','default_updated','default_stage_target','default_final_target','default_failure_before','default_failure_after') and (stage or mode not in ('false_success','wrong_bytes','failure_after','race','signal','default_failure_after')):
        replacement=root/'bin/new-ph'
        replacement.write_text('#!/bin/sh\nprintf "ph %s\\n"\n' % os.environ['FAKE_NEXT'])
        replacement.chmod(0o700)
        replacement.replace(root/'bin/ph')
        data=json.loads(os.environ['FAKE_NEW_META'])
        if (mode=='default_stage_target' and stage) or (mode=='default_final_target' and not stage):
            next(iter(data['installs'].values()))['target']='aarch64-unknown-linux-gnu'
        (root/'.crates2.json').write_text(json.dumps(data))
    elif mode=='false_success' and not stage:
        (root/'.crates2.json').write_text(os.environ['FAKE_NEW_META'])
    elif mode in ('wrong_bytes','failure_after','race','default_failure_after') and not stage:
        replacement=root/'bin/new-ph'
        replacement.write_text('#!/bin/sh\nprintf "ph %s\\n"\n# unexpected bytes\n' % os.environ['FAKE_NEXT'])
        replacement.chmod(0o700)
        replacement.replace(root/'bin/ph')
        (root/'.crates2.json').write_text(os.environ['FAKE_NEW_META'])
        if mode in ('failure_after','race','signal','default_failure_after'): sys.exit(7)
'''
for mode in ('default_updated','default_current','default_wrong_target','default_stage_target','default_final_target','default_failure','default_failure_before','default_failure_after','explicit_wrong_version','explicit_wrong_target','updated','current','mismatch','failure','false_success','wrong_source','declined','non_tty','advance','wrong_bytes','failure_after','race','signal','alias'):
    with tempfile.TemporaryDirectory(prefix='ph-update-test-') as tmp:
        base=pathlib.Path(tmp)
        if mode=='alias':
            (base/'physical').mkdir()
            (base/'alias').symlink_to(base/'physical', target_is_directory=True)
            home=base/'alias/home'
        else: home=base/'home'
        root=home/'cargo'; fake=home/'fake'; (root/'bin').mkdir(parents=True); fake.mkdir()
        exe=root/'bin/ph'; shutil.copy2(binary,exe); original=exe.read_bytes()
        initial=meta(source='path+file:///local' if mode=='wrong_source' else registry)
        (root/'.crates2.json').write_text(json.dumps(initial))
        (root/'.crates.toml').write_bytes(b'[v1]\nfixture = []\n')
        cargo=fake/'cargo'; cargo.write_text(FAKE.replace('#!/usr/bin/python3', '#!'+sys.executable, 1)); cargo.chmod(0o700)
        env=os.environ.copy(); env.update(HOME=str(home),CARGO_HOME=str(root),PATH=str(fake),FAKE_LOG=str(home/'log'),FAKE_MODE=('updated' if mode in ('declined','non_tty','alias') else mode),FAKE_ORIGINAL=str(exe),FAKE_META=json.dumps(meta()),FAKE_NEW_META=json.dumps(meta(next_version)),FAKE_NEXT=next_version,FAKE_READY=str(home/'ready'),FAKE_GATE=str(home/'gate'),FAKE_REGISTRY=str(home/'registry'))
        master,slave=pty.openpty()
        try:
            stdin = subprocess.DEVNULL if mode=='non_tty' else slave
            process=subprocess.Popen([str(exe),'update'],env=env,stdin=stdin,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
            os.close(slave); slave=None
            if mode!='non_tty': os.write(master, b'n\n' if mode=='declined' else b'y\n')
            if mode in ('signal','race'):
                deadline=time.monotonic()+20
                while not (home/'ready').exists() and time.monotonic()<deadline: time.sleep(.02)
                assert (home/'ready').exists(), 'final Cargo was not reached'
                if mode=='signal': os.killpg(process.pid, signal.SIGTERM)
                else:
                    replacement=root/'bin/external-ph'
                    replacement.write_bytes(b'external installer replacement')
                    replacement.replace(exe)
                    (home/'gate').write_text('continue')
            stdout,stderr=process.communicate(timeout=30)
            result=subprocess.CompletedProcess([str(exe),'update'],process.returncode,stdout,stderr)
        finally:
            os.close(master)
            if slave is not None: os.close(slave)
        log=[json.loads(line) for line in (home/'log').read_text().splitlines()] if (home/'log').exists() else []
        assert result.stdout==b'',(mode,result.stdout)
        if mode in ('updated','advance','alias','default_updated'):
            assert b'If unsure, answer no and use your original installer.' in result.stderr,(mode,result.stderr)
            assert result.returncode==0 and f'updated: ph {next_version}'.encode() in result.stderr and len(log)==(4 if mode=='default_updated' else 3),(mode,result.stderr,log)
        elif mode in ('current','default_current'):
            assert result.returncode==0 and f'ph {current} is already current'.encode() in result.stderr and len(log)==(3 if mode=='default_current' else 2),(mode,result.stderr,log)
        else:
            assert result.returncode!=0,(mode,result.stderr,log)
            if mode in ('mismatch','failure','wrong_source','declined','non_tty','default_wrong_target','default_stage_target','default_failure','default_failure_before','explicit_wrong_version','explicit_wrong_target'):
                assert exe.read_bytes()==original
            assert len(log)==(0 if mode=='wrong_source' else 1 if mode in ('failure','explicit_wrong_version','explicit_wrong_target') else 2 if mode in ('mismatch','declined','non_tty','default_wrong_target','default_failure') else 4 if mode in ('default_final_target','default_failure_before','default_failure_after') else 3),(mode,log)
            if mode in ('default_failure','default_failure_before','default_failure_after'):
                assert b'Cargo failed' in result.stderr and b'check any Cargo output above and your Cargo setup, or use the original installer' in result.stderr,(mode,result.stderr)
            if mode=='default_stage_target':
                assert b'Cargo selected a different compilation target; no update was started' in result.stderr,(mode,result.stderr)
            if mode in ('explicit_wrong_version','explicit_wrong_target'):
                assert b'Cargo rebuilt a different version or target; no update was started' in result.stderr,(mode,result.stderr)
        if mode in ('updated','advance','alias','default_updated','default_final_target'):
            assert log[-1][log[-1].index('--version')+1] == f'={next_version}'
        if mode=='advance': assert (home/'registry').read_text()=='99.0.2'
        if mode=='race': assert exe.read_bytes()==b'external installer replacement'
        if mode=='signal': assert exe.read_bytes()==b'partial installation'
        backups=list(root.glob('ph-update-backup-*'))
        if mode in ('false_success','wrong_bytes','failure_after','race','signal','default_final_target','default_failure_before','default_failure_after'):
            assert len(backups)==1 and (backups[0]/'ph').read_bytes()==original and (backups[0]/'.crates2.json').read_bytes()==json.dumps(initial).encode() and (backups[0]/'.crates.toml').read_bytes()==b'[v1]\nfixture = []\n' and (b'The installation may have changed' in result.stderr if mode!='signal' else b'recovery copy:' in result.stderr),(mode,result.stderr,backups)
        else: assert not backups,(mode,backups)
        for args in log:
            assert '--force' not in args and '--locked' in args and '--no-default-features' in args and args[args.index('--features')+1]=='tui' and args[args.index('--registry')+1]=='crates-io',(mode,args)
        if mode.startswith('default_') or mode=='mismatch':
            assert '--target' in log[0] and all('--target' not in args for args in log[1:]),(mode,log)
        else:
            assert all('--target' in args for args in log),(mode,log)
        print(f'PASS {mode}: cargo invocations={len(log)}, exit={result.returncode}, stdout empty')
