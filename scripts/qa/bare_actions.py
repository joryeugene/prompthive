#!/usr/bin/env python3
"""Exercise bare-ph actions in an isolated controlling PTY, never a desktop clipboard."""
import os, sys
if os.name != 'posix':
    print('EXCEPTION: bare action PTY checks require POSIX; native Windows is not covered.')
    raise SystemExit(0)
import fcntl, pty, select, signal, struct, tempfile, termios, time
from pathlib import Path
from terminal_session import TerminalChild, terminal_modes

binary = str(Path(sys.argv[1]).resolve())
for invocation in ([], ['tui'], ['tui', 'review']):
  for action, keys in [('show', b'\t\x1b[C\r'), ('edit', b'\t\x1b[C\x1b[C\r'), ('cancel', b'\t\x1b[C\x1b')]:
      with tempfile.TemporaryDirectory(prefix='ph-actions-') as temp:
          root = Path(temp)
          prompt = root / 'store/prompts/review.md'
          prompt.parent.mkdir(parents=True)
          original = 'Review this safely.\n\x1b]52;c;YWJj\x07'
          prompt.write_text(original)
          editor = root / 'editor'
          editor.write_text('#!/bin/sh\nprintf "Edited safely.\\n" > "$1"\n')
          editor.chmod(0o700)
          env = os.environ.copy()
          env.update(PROMPTHIVE_BASE_DIR=str(root/'store'), TERM='xterm-256color', EDITOR=str(editor), NO_COLOR='1')
          for k in ('DISPLAY', 'WAYLAND_DISPLAY', 'VISUAL'):
              env.pop(k, None)
          master, slave = pty.openpty()
          fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 18, 40, 0, 0))
          before = terminal_modes(slave)
          child = TerminalChild([binary, *invocation], slave=slave, stdin=slave, stdout=slave, stderr=slave, cwd=root, env=env)
          output = bytearray()
          sent = False
          try:
              deadline = time.monotonic() + 8
              while child.poll() is None and time.monotonic() < deadline:
                  if select.select([master], [], [], .05)[0]:
                      output.extend(os.read(master, 65536))
                  if not sent and b'preview' in output:
                      os.write(master, keys)
                      sent = True
              if child.poll() is None:
                  child.send_signal(signal.SIGKILL)
                  raise RuntimeError(f'{action}: timed out')
              child.wait(timeout=2)
              while select.select([master], [], [], 0)[0]:
                  try:
                      chunk = os.read(master, 65536)
                      if not chunk: break
                      output.extend(chunk)
                  except OSError:
                      break
              restored = terminal_modes(slave) == before
              if not sent or not restored:
                  raise AssertionError(f'{action}: sent={sent}, restored={restored}')
              if action == 'show':
                  assert b'\x1b[?1049l' in output, 'alternate screen did not close'
                  shown = output.rsplit(b'\x1b[?1049l', 1)[1]
                  assert child.returncode == 0 and b'Review this safely.' in shown and prompt.read_text() == original
                  assert b'\x1b]52;' not in shown and b'\\u{1b}]52;c;YWJj\\u{7}' in shown, shown
              elif action == 'edit':
                  assert child.returncode == 0 and 'Edited safely.' in prompt.read_text()
              else:
                  assert child.returncode == 130 and prompt.read_text() == original
              print(f'PASS {invocation or ["bare"]} {action}: exit={child.returncode}, terminal restored, expected fixture state')
          finally:
              os.close(master); os.close(slave); child.close()
