#!/usr/bin/env python3
"""Observe v0.4 terminal actions and actual output in disposable stores only."""
import argparse, hashlib, json, os, pathlib, re, select, signal, struct
import subprocess, sys, tempfile, termios, time, fcntl, pty
from vt_decode import VtScreen
from terminal_session import TerminalChild, terminal_modes

P = argparse.ArgumentParser(description=__doc__)
P.add_argument('binary', type=pathlib.Path)
P.add_argument('--report', type=pathlib.Path)
P.add_argument('--capture-dir', type=pathlib.Path)
P.add_argument('--case', action='append', default=[])
A = P.parse_args()
BINARY = A.binary.resolve(strict=True)

def hashes(store):
    return {str(p.relative_to(store)):hashlib.sha256(p.read_bytes()).hexdigest()
            for p in store.rglob('*') if p.is_file()}

def case(name):
    with tempfile.TemporaryDirectory(prefix='ph-interactive-') as directory:
        root=pathlib.Path(directory); store=root/'store'; (store/'prompts').mkdir(parents=True)
        templates={'explain':'---\nvars: {lang: null, tone: null}\n---\nG [{lang}]|[{tone}]|[{input}]',
                   'plain':'PLAIN [{input}]',
                   'defaulted':'---\nvars: {topic: null, tone: brief}\n---\nD [{topic}]|[{tone}]',
                   'clock':'---\nvars: {topic: null}\n---\nSNAP={uuid}\nVALUE={topic}',
                   'legacy':'---\nid: retained-id\ncustom: !keep {nested: true}\n---\nOld note\n',
                   'broken':'---\nbad: [\n---\nOld malformed note',
                   'env_fail':'---\nvars: {topic: null}\n---\nFail {env:PROMPTHIVE_QA_MISSING_ENV_0V4} {topic}'}
        for id,body in templates.items(): (store/'prompts'/f'{id}.md').write_text(body,encoding='utf-8')
        if name=='with_directives':
            (store/'prompts/directive.md').write_text('---\nvars: {policy: null}\n---\nRULE [{policy}]',encoding='utf-8')
        long_name='work/'+('identity-'*7)+'longform'
        if name=='long_identity':
            path=store/'banks'/(long_name+'.md');path.parent.mkdir(parents=True);path.write_text(templates['explain'])
        helper=root/'editor.py'
        helper.write_text("import pathlib,sys\np=pathlib.Path(sys.argv[-1])\nmode=sys.argv[1]\nif mode=='write': p.write_text('New note café\\n\\n',encoding='utf-8')\nelif mode=='edit': p.write_text(p.read_text().replace('Old','New'),encoding='utf-8')\nelif mode=='repair': p.write_text('Repaired note\\n',encoding='utf-8')\nelif mode=='fail': p.write_text('staged only');sys.exit(9)\nelif mode=='cancel': sys.exit(130)\n",encoding='utf-8')
        editor_mode='edit' if name=='edit_search' else 'repair' if name=='edit_repair' else 'fail' if name=='new_failure' else 'cancel' if name=='new_cancel' else 'noop' if name=='new_empty' else 'write'
        import shlex
        env=os.environ.copy(); env.update(PROMPTHIVE_BASE_DIR=str(store), TERM='xterm-256color',
            XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),XDG_STATE_HOME=str(root/'state'),
            XDG_CACHE_HOME=str(root/'cache'),XDG_RUNTIME_DIR=str(root/'runtime'),
            EDITOR=shlex.quote(sys.executable)+' '+shlex.quote(str(helper))+' '+editor_mode)
        for k in ['DISPLAY','WAYLAND_DISPLAY','NO_COLOR','BASH_ENV','ENV','PROMPTHIVE_QA_MISSING_ENV_0V4']: env.pop(k,None)
        width,height=(40,18) if name in ['guided_narrow','long_identity'] else (120,40) if name=='guided_wide' else (80,24)
        master,slave=pty.openpty(); fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',height,width,0,0))
        before_modes=terminal_modes(slave); before=hashes(store); screen=VtScreen(width,height); raw=bytearray(); frames=[]; events=[]
        args=['u','-f','result.txt']; query='exp'; expected=b'G [Rust]|[brief]|[]'; expected_code=0
        if name in ['edit_search','edit_repair']: args=['e'];query='legacy' if name=='edit_search' else 'broken'
        if name.startswith('new_'):args=['n','notes/today']
        if name in ['new_failure']: expected_code=1
        if name in ['new_empty','new_cancel','cancel_fields','cancel_preview','ctrl_c','sigterm']:expected_code=130
        if name=='defaults': query='defaulted';expected=b'D [Rust]|[brief]'
        if name=='explicit_empty': query='defaulted';expected=b'D []|[brief]'
        if name=='literal_multiline': query='defaulted';expected='D [  café 🦀\n{env:NOT_EXPANDED} $()\n]|[brief]'.encode()
        if name=='supplied_fast':args=['u','-v','lang=Rust','-v','tone=brief','-f','result.txt']
        if name=='one_supplied':args=['u','-v','lang=Rust','-f','result.txt']
        if name=='default_fast':args=['u','-v','topic=Rust','-f','result.txt'];query='defaulted';expected=b'D [Rust]|[brief]'
        if name=='direct_missing':args=['u','explain','-f','result.txt'];expected_code=2
        if name=='plain_fast':query='plain';expected=b'PLAIN []'
        if name=='derived_destination':args=['u','-f']
        if name=='frozen_context':query='clock'
        if name=='render_failure':query='env_fail';expected_code=2
        if name=='long_identity':query='longform'
        terminal_payload=name in ['bare_guided','t_guided']
        if name=='bare_guided':args=[]
        if name=='t_guided':args=['t','exp'];query=''
        if name=='unknown_picker':args=['u','exp','-f','result.txt'];query=''
        if name=='with_directives':args=['u','--with','directive','-f','result.txt'];expected=b'RULE [safe]\n\nG [Rust]|[brief]|[]'
        piped=name=='piped_context'
        if piped:expected=b'G [Rust]|[brief]|[PIPE {literal}\n]'
        stderr=(root/'stderr').open('wb')
        child=TerminalChild([str(BINARY),*args],slave=slave,stdin=subprocess.PIPE if piped else slave,stdout=slave,stderr=stderr,
            env=env,cwd=root)
        if piped:child.stdin.write(b'PIPE {literal}\n');child.stdin.close()
        def drain(wait=.05):
            if select.select([master],[],[],wait)[0]:
                data=os.read(master,65536);raw.extend(data);screen.feed(data)
        def wait(text):
            deadline=time.monotonic()+8
            while time.monotonic()<deadline:
                drain()
                if text in screen.text():
                    # Settle a complete frame instead of accepting half a draw.
                    time.sleep(.03)
                    while select.select([master],[],[],0)[0]:drain(0)
                    return
                if child.poll() is not None:raise AssertionError('exited before '+text+': '+(root/'stderr').read_text())
            raise AssertionError('timeout waiting for '+text+'; '+screen.text())
        def key(value):events.append(value.hex());os.write(master,value)
        def capture(label):
            frames.append({'label':label,'text':screen.text(),'width':screen.columns,'height':screen.rows})
        failure=None; after_modes=None; actual=None
        try:
            if not name.startswith('new_') and name!='direct_missing':
                wait('Find');capture('picker');key(query.encode()+b'\r')
                if name in ['edit_search','edit_repair','supplied_fast','plain_fast','default_fast']:pass
                else:
                    wait('Fill required values');capture('fields')
                    if name=='cancel_fields':key(b'\x1b')
                    elif name=='ctrl_c':key(b'\x03')
                    elif name=='sigterm':child.send_signal(signal.SIGTERM)
                    else:
                        if name=='literal_multiline':key(b'\x1b[200~'+expected.decode().split('D [',1)[1].rsplit(']|[brief]',1)[0].encode()+b'\x1b[201~\r')
                        elif name=='explicit_empty':key(b'\r')
                        elif name in ['defaults','frozen_context','render_failure']:key(b'Rust\r')
                        elif name=='one_supplied':key(b'brief\r')
                        elif name=='with_directives':key(b'Rust\tsafe\tbrief\r')
                        else:key(b'Rust\tbrief\r')
                        if name!='render_failure':wait('Preview rendered prompt');capture('rendered')
                        if name=='long_identity':assert long_name in screen.text().replace('\n','').replace(' ','')
                        if name=='render_failure':pass
                        elif name=='cancel_preview':key(b'\x1b')
                        elif name=='resize':
                            screen.resize(40,18);fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',18,40,0,0));child.send_signal(signal.SIGWINCH);wait('Preview rendered prompt');capture('resized');key(b'\r')
                        elif name=='tiny_blocked':
                            screen.resize(24,8);fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',8,24,0,0));child.send_signal(signal.SIGWINCH);wait('too small');key(b'\r');time.sleep(.1);drain(0);assert child.poll() is None and not (root/'result.txt').exists();key(b'\x1b');expected_code=130
                        else:key(b'\r')
            deadline=time.monotonic()+8
            while child.poll() is None and time.monotonic()<deadline:drain()
            assert child.poll() is not None,'process hung after final action'
            for _ in range(3):drain(.02)
            assert child.returncode==expected_code,(child.returncode,(root/'stderr').read_text())
            after_modes=terminal_modes(slave)
            assert after_modes==before_modes,'terminal attributes changed'
            assert not screen.unsupported,screen.unsupported
            if expected_code!=0:assert hashes(store)==before and not (root/'result.txt').exists()
            elif name.startswith('new_'):
                saved=(store/'banks/notes/today.md').read_text();assert saved.endswith('New note café\n\n')
            elif name in ['edit_search','edit_repair']:
                id='legacy' if name=='edit_search' else 'broken';saved=(store/'prompts'/f'{id}.md').read_text()
                assert saved==(templates[id].replace('Old','New') if name=='edit_search' else 'Repaired note\n')
                assert all(hashes(store).get(k)==v for k,v in before.items() if k!=f'prompts/{id}.md')
            else:
                if terminal_payload:
                    assert b'\x1b[?1049l' in raw,'alternate screen was not restored'
                    actual=bytes(raw).rsplit(b'\x1b[?1049l',1)[1]
                else:
                    output=root/('explain.md' if name=='derived_destination' else 'result.txt');actual=output.read_bytes()
                if name=='frozen_context':
                    uuid=re.search(rb'SNAP=([0-9a-f-]{36})',actual).group(1).decode();shown=next(f['text'] for f in frames if f['label']=='rendered').replace('\n','').replace(' ','');assert uuid in shown;assert actual.endswith(b'\nVALUE=Rust')
                else:assert actual==expected,(actual,expected)
                assert hashes(store)==before,'rendering changed saved prompts'
            if name in ['plain_fast','supplied_fast','default_fast']:assert b'Fill required values' not in raw and b'Preview rendered prompt' not in raw
        except Exception as error:failure=repr(error)
        finally:
            if after_modes is None:
                try:after_modes=terminal_modes(slave)
                except termios.error as error:failure=failure or repr(error)
            child.close()
            stderr.close();os.close(master);os.close(slave)
        result={'case':name,'argv':args,'passed':failure is None,'error':failure,'exit':child.returncode,'events_hex':events,
                'injected_bytes':sum(len(bytes.fromhex(v)) for v in events),'frames':frames,'terminal_restored':after_modes==before_modes,
                'observed_payload_hex':None if actual is None else actual.hex(),'stderr':(root/'stderr').read_text(encoding='utf-8',errors='replace')}
        if A.capture_dir:
            A.capture_dir.mkdir(parents=True,exist_ok=True);(A.capture_dir/(name+'.terminal.bin')).write_bytes(raw)
            for frame in frames:(A.capture_dir/(name+'.'+frame['label']+'.screen.txt')).write_text(frame['text'],encoding='utf-8')
        return result

names=A.case or ['guided_two','guided_narrow','guided_wide','long_identity','defaults','explicit_empty','literal_multiline','piped_context','frozen_context',
    'bare_guided','t_guided','unknown_picker','with_directives',
    'derived_destination','supplied_fast','one_supplied','default_fast','direct_missing','render_failure','plain_fast','cancel_fields','cancel_preview','ctrl_c','sigterm','resize','tiny_blocked',
    'edit_search','edit_repair','new_editor','new_empty','new_failure','new_cancel']
results=[]
for name in names:
    row=case(name);results.append(row)
    print(('PASS' if row['passed'] else 'FAIL')+': '+row['case']+(' '+str(row['error']) if row['error'] else ''),flush=True)
report={'binary':str(BINARY),'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'passed':all(c['passed'] for c in results),
    'cases':results,'limits':'POSIX PTYs and synthetic stores/EDITOR only; no physical desktop clipboard or human acceptance claim.'}
if A.report:A.report.parent.mkdir(parents=True,exist_ok=True);A.report.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
raise SystemExit(0 if report['passed'] else 1)
