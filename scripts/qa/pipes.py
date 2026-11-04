#!/usr/bin/env python3
"""Observable Bash pipelines and bounded PTY cancellation, using only stdlib."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(store):
    return {str(p.relative_to(store)): digest(p) for p in store.rglob('*') if p.is_file()}


def isolated_environment(root, binary):
    env = os.environ.copy()
    env.update(PROMPTHIVE_BASE_DIR=str(root / 'store'),
               PROMPTHIVE_TEST_BIN=binary.as_posix(), TERM='dumb', NO_COLOR='1')
    for name in ['CONFIG', 'DATA', 'STATE', 'CACHE', 'RUNTIME']:
        env['XDG_' + name + ('_DIR' if name == 'RUNTIME' else '_HOME')] = str(root / name.lower())
    for key in ['DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY', 'BASH_ENV', 'ENV',
                'EDITOR', 'VISUAL', 'PROMPTHIVE_TEST_MODE']:
        env.pop(key, None)
    for key in list(env):
        if key.startswith('BASH_FUNC_'):
            env.pop(key)
    return env


SHELL_SETUP = 'ph() { "$PROMPTHIVE_TEST_BIN" "$@"; }; '


def bounded_shell(bash, command, root, env, timeout):
    child = subprocess.Popen([bash, '--noprofile', '--norc', '-o', 'pipefail', '-c', SHELL_SETUP + command],
                             cwd=root, env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             start_new_session=os.name == 'posix')
    try:
        stdout, stderr = child.communicate(timeout=timeout)
        return subprocess.CompletedProcess(child.args, child.returncode, stdout, stderr)
    except subprocess.TimeoutExpired:
        # A Bash timeout must also stop its owned pipeline children, which can
        # otherwise keep captured pipe handles open after Bash itself is killed.
        if os.name == 'posix':
            os.killpg(child.pid, signal.SIGKILL)
        else:
            subprocess.run(['taskkill', '/PID', str(child.pid), '/T', '/F'],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3)
            if child.poll() is None:
                child.kill()
        try:
            child.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            child.stdout.close()
            child.stderr.close()
        raise


def pipeline_cases(binary, bash, timeout):
    results = []
    with tempfile.TemporaryDirectory(prefix='ph-pipes-') as directory:
        root = Path(directory)
        store = root / 'store'
        (store / 'prompts').mkdir(parents=True)
        (store / 'banks/auth').mkdir(parents=True)
        fixtures = {'review': 'Review [{input}]', 'rules': 'Rules {input}',
                    'required': '---\nvars: {lang: null}\n---\n{lang}: {input}',
                    'defaults': '---\nvars: {tone: brief}\n---\n{tone}: {input}',
                    'huge': 'x' * 8_000_000}
        for name, text in fixtures.items():
            (store / 'prompts' / (name + '.md')).write_text(text, encoding='utf-8')
        for name in ['basic', 'bearer']:
            (store / 'banks/auth' / (name + '.md')).write_text(name, encoding='utf-8')
        protected = root / 'protected.txt'
        protected.write_bytes(b'Keep the original file.\n')
        env = isolated_environment(root, binary)
        cases = [
            ('stdin-use-unicode-whitespace', "printf '  caf\\303\\251\\n\\n' | ph u review", 0, 'Review [  café\n\n]'.encode()),
            ('composition', 'printf ctx | ph c rules,review', 0, b'Review [Rules ctx]'),
            ('default-variable', 'printf ctx | ph u defaults', 0, b'brief: ctx'),
            ('explicit-variable', 'printf ctx | ph u required -v lang=Rust', 0, b'Rust: ctx'),
            ('literal-inserted-data', "printf '%s' '{env:NEVER_EXPAND} $(not-code)' | ph u review", 0, b'Review [{env:NEVER_EXPAND} $(not-code)]'),
            ('empty-input', "printf '' | ph u review", 0, b'Review []'),
            ('save-keeps-stdout-empty', 'printf ctx | ph u review -s notes/run', 0, b''),
            ('redirect-exact-bytes', 'printf ctx | ph u review > result.txt && cat result.txt', 0, b'Review [ctx]'),
            ('missing-required-value', 'printf ctx | ph u required', 2, b''),
            ('failed-render-preserves-file', 'printf ctx | ph u required -f protected.txt --force', 2, b''),
            ('ambiguous-name', 'printf ctx | ph u auth/b', 2, b''),
            ('both-inputs', 'printf pipe | ph u review positional', 2, b''),
            ('non-tty-empty-help', 'ph </dev/null', 0, None),
            ('non-tty-context-needs-name', 'printf ctx | ph', 2, b''),
            ('non-tty-picker-fails', 'ph t', 2, b''),
            ('missing-name', 'printf ctx | ph u missing', 2, b''),
            ('quiet-broken-pipe', 'ph s huge | head -c 1', None, b'x'),
        ]
        for name, command, expected_status, expected_stdout in cases:
            before = snapshot(store)
            started = time.monotonic()
            try:
                result = bounded_shell(bash, command, root, env, timeout)
                checks = {
                    'exit': result.returncode in [0, 141] if expected_status is None else result.returncode == expected_status,
                    'stdout': b'ph u review' in result.stdout if expected_stdout is None else result.stdout == expected_stdout,
                    'protected-file': protected.read_bytes() == b'Keep the original file.\n',
                    'prior-store-bytes': all(snapshot(store).get(p) == value for p, value in before.items()),
                }
                if expected_status == 2:
                    checks['stderr-diagnostic'] = bool(result.stderr)
                elif name == 'save-keeps-stdout-empty':
                    checks['status-on-stderr'] = b'Saved notes/run.' in result.stderr
                    checks['saved-name-exists'] = (store / 'banks/notes/run.md').is_file()
                    saved = subprocess.run([str(binary), 's', 'notes/run', '-q'], cwd=root, env=env,
                                           stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout)
                    checks['saved-file'] = saved.returncode == 0 and saved.stdout == b'Review [ctx]' and saved.stderr == b''
                else:
                    checks['stderr-empty'] = result.stderr == b''
                if name != 'save-keeps-stdout-empty':
                    checks['store-unchanged'] = snapshot(store) == before
                results.append({'case': name, 'command': command, 'exit': result.returncode,
                                'stdout_hex': result.stdout.hex(), 'stderr': result.stderr.decode(errors='replace'),
                                'checks': checks, 'passed': all(checks.values()),
                                'elapsed_seconds': round(time.monotonic() - started, 3), 'timeout_seconds': timeout})
            except subprocess.TimeoutExpired:
                results.append({'case': name, 'command': command, 'passed': False,
                                'error': 'Pipeline exceeded its no-hang deadline', 'timeout_seconds': timeout})
    return results


def cancellation_case(binary, bash, timeout, key):
    # Python on native Windows has no PTY API. That is an explicit exception,
    # not evidence of Windows terminal or desktop clipboard compatibility.
    import fcntl
    import pty
    import select
    import struct
    import termios
    from terminal_session import TerminalChild, terminal_modes
    with tempfile.TemporaryDirectory(prefix='ph-cancel-') as directory:
        root = Path(directory)
        store = root / 'store'
        (store / 'prompts').mkdir(parents=True)
        (store / 'prompts/review.md').write_text('Review {input}', encoding='utf-8')
        before_files = snapshot(store)
        env = isolated_environment(root, binary)
        env['TERM'] = 'xterm-256color'
        env.pop('NO_COLOR', None)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
        before_modes = terminal_modes(slave)
        child = TerminalChild([bash, '--noprofile', '--norc', '-o', 'pipefail', '-c', SHELL_SETUP + 'printf ctx | ph'],
                              slave=slave, cwd=root, env=env, stdin=slave, stdout=slave, stderr=slave)
        output = bytearray()
        sent = False
        timed_out = False
        try:
            deadline = time.monotonic() + timeout
            while child.poll() is None and time.monotonic() < deadline:
                if select.select([master], [], [], .05)[0]:
                    output.extend(os.read(master, 65536))
                if not sent and b'preview' in output:
                    os.write(master, key)
                    sent = True
            if child.poll() is None:
                timed_out = True
                child.send_signal(signal.SIGKILL)
            child.wait(timeout=2)
            checks = {'picker-observed': sent, 'exit130': child.returncode == 130,
                      'no-hang': not timed_out, 'store-unchanged': snapshot(store) == before_files,
                      'terminal-restored': terminal_modes(slave) == before_modes}
            return {'case': 'piped-context-' + ('escape' if key == b'\x1b' else 'ctrl-c'),
                    'command': 'printf ctx | ph', 'injected_key_hex': key.hex(),
                    'exit': child.returncode, 'checks': checks, 'passed': all(checks.values()),
                    'terminal_output_tail': output[-8000:].decode('utf-8', errors='replace') if not all(checks.values()) else None,
                    'terminal_sha256': hashlib.sha256(output).hexdigest(), 'timeout_seconds': timeout}
        finally:
            os.close(master)
            os.close(slave)
            child.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', nargs='?', help='Executable to test; default: ph on PATH')
    parser.add_argument('--bash', default=shutil.which('bash'), help=argparse.SUPPRESS)
    parser.add_argument('--headless', action='store_true', help='Expect a build without the terminal picker')
    parser.add_argument('--report', type=Path, help='Write the exact executable identity and observations as JSON')
    parser.add_argument('--timeout', type=float, default=8)
    args = parser.parse_args()
    name = args.binary or shutil.which('ph')
    if not name or not args.bash:
        parser.error('provide an existing ph executable and Bash')
    binary = Path(name).resolve(strict=True)
    cases = pipeline_cases(binary, args.bash, args.timeout)
    exceptions = []
    if args.headless:
        exceptions.append('PTY cancellation is inapplicable to the explicitly selected no-TUI build.')
    elif os.name != 'posix':
        exceptions.append('Native Windows Python has no PTY API; terminal cancellation remains a separate native acceptance requirement.')
    else:
        for key in [b'\x1b', b'\x03']:
            cases.append(cancellation_case(binary, args.bash, args.timeout, key))
    report = {'binary': str(binary), 'binary_sha256': digest(binary), 'passed': all(c['passed'] for c in cases),
              'headless': args.headless, 'cases': cases, 'exceptions': exceptions,
              'isolation': 'Disposable store and XDG configuration; display variables removed. No real prompts or clipboard accessed.'}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    for case in cases:
        print(('PASS' if case['passed'] else 'FAIL') + ': ' + case['case'])
        if not case['passed']:
            print(json.dumps(case, indent=2))
    for exception in exceptions:
        print('EXCEPTION: ' + exception)
    print(f"{sum(c['passed'] for c in cases)}/{len(cases)} real-shell cases passed for {binary}")
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
