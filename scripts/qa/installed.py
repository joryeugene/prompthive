#!/usr/bin/env python3
"""Verify clean default/headless installs from the actual locked Cargo package."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import tempfile


def extract_package(archive, destination, prefix):
    # Cargo archives need regular source files only. Do not trust archive paths
    # or retain links while installing the package we are about to exercise.
    with tarfile.open(archive) as source:
        for member in source:
            path = PurePosixPath(member.name)
            if (path.is_absolute() or '..' in path.parts or not path.parts
                    or path.parts[0] != prefix or '\\' in member.name or ':' in member.name):
                raise RuntimeError('Unsafe source path in Cargo package: ' + member.name)
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as incoming, target.open('wb') as outgoing:
                    shutil.copyfileobj(incoming, outgoing)
            else:
                raise RuntimeError('Cargo package contains a non-regular source entry: ' + member.name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bash', default=shutil.which('bash'), help=argparse.SUPPRESS)
    parser.add_argument('--report', type=Path, help='Save package identity and installed executable observations')
    args = parser.parse_args()
    if not args.bash:
        parser.error('Bash is required for the real-shell contracts')
    repo = Path(__file__).resolve().parents[2]
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked', '--offline'], cwd=repo))
    package = next(p for p in metadata['packages'] if Path(p['manifest_path']).resolve() == repo / 'Cargo.toml')
    subprocess.run(['cargo', 'package', '--locked', '--offline'], cwd=repo, check=True)
    prefix = package['name'] + '-' + package['version']
    target = Path(metadata['target_directory'])
    archive = target / 'package' / (prefix + '.crate')
    archive_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
    observations = []
    with tempfile.TemporaryDirectory(prefix='ph-installed-contract-') as directory:
        temporary = Path(directory)
        extract_package(archive, temporary / 'source', prefix)
        source = temporary / 'source' / prefix
        for headless in [False, True]:
            mode = 'headless' if headless else 'default'
            install = temporary / mode
            environment = os.environ.copy()
            # Respect the caller's Cargo target location without overwriting its
            # regular debug executable while switching installed feature sets.
            environment['CARGO_TARGET_DIR'] = str(target / 'installed-contract')
            command = ['cargo', 'install', '--path', str(source), '--locked', '--offline', '--root', str(install)]
            if headless:
                command.append('--no-default-features')
            subprocess.run(command, cwd=repo, env=environment, check=True)
            binary = install / 'bin' / ('ph.exe' if os.name == 'nt' else 'ph')
            version = subprocess.check_output([str(binary), '--version'], stdin=subprocess.DEVNULL, timeout=8).decode().strip()
            if version != 'ph ' + package['version']:
                raise RuntimeError(f'Installed version mismatch: {version!r}')
            result_path = temporary / (mode + '.json')
            check = [sys.executable, str(repo / 'scripts/qa/pipes.py'), '--bash', args.bash,
                     str(binary), '--report', str(result_path)]
            if headless:
                check.append('--headless')
            subprocess.run(check, cwd=repo, check=True)
            observation = json.loads(result_path.read_text(encoding='utf-8'))
            if not headless and os.name == 'posix':
                interactive_path = temporary / 'interactive.json'
                subprocess.run([sys.executable, str(repo / 'scripts/qa/interactive.py'), str(binary),
                                '--report', str(interactive_path)], cwd=repo, check=True)
                observation['interactive'] = json.loads(interactive_path.read_text(encoding='utf-8'))
            else:
                observation['interactive_exception'] = ('No-TUI build has no interactive flow.' if headless else
                    'Native Windows Python lacks POSIX PTYs; native terminal acceptance remains separate.')
            observation['mode'] = mode
            observation['version'] = version
            observation['install_argv'] = command
            observations.append(observation)
    report = {'package': str(archive), 'package_sha256': archive_hash,
              'package_name': package['name'], 'package_version': package['version'],
              'passed': all(o['passed'] for o in observations), 'installed': observations,
              'limits': 'Temporary source installs, not registry publication or physical desktop clipboard acceptance.'}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f"PASS: package {package['name']} {package['version']} sha256={archive_hash}")
    for observation in observations:
        print(f"PASS: installed {observation['mode']} sha256={observation['binary_sha256']}")
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
