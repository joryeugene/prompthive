"""Real PTY observer regressions; no application or desktop data is accessed."""
import os
import sys
if os.name != 'posix':
    print('EXCEPTION: native Windows has no POSIX PTY observer.')
    raise SystemExit(0)
import pty
import select
import signal
import subprocess
import termios
import time
import unittest
from terminal_session import TerminalChild, terminal_modes


class TerminalSessionContract(unittest.TestCase):
    def exercise(self, source, interact=None, piped=False):
        master, slave = pty.openpty()
        before = terminal_modes(slave)
        child = None
        try:
            child = TerminalChild([sys.executable, '-c', source], slave=slave,
                stdin=subprocess.PIPE if piped else slave, stdout=slave,
                stderr=slave, cwd=os.getcwd(), env=os.environ.copy())
            if interact:
                interact(child, master)
            result = child.wait(timeout=3)
            # A live owner is required here on Darwin, and must never itself
            # repair the modes that the tested process left behind.
            self.assertIsNone(child.supervisor.poll())
            after = terminal_modes(slave)
            if before != after:
                print('PTY mode difference:', [(i, a, b) for i, (a, b) in enumerate(zip(before, after)) if a != b], file=sys.stderr)
            return result, before == after
        finally:
            if child is not None:
                child.close()
            os.close(master)
            os.close(slave)

    def test_restored_modes_pass(self):
        self.assertEqual(self.exercise('import termios,tty; old=termios.tcgetattr(1); tty.setraw(1); termios.tcsetattr(1,termios.TCSANOW,old)'), (0, True))

    def test_identity_roundtrip_preserves_fixture(self):
        self.assertEqual(self.exercise('import termios; old=termios.tcgetattr(1); termios.tcsetattr(1,termios.TCSANOW,old)'), (0, True))

    def test_dirty_modes_are_rejected_after_successful_exit(self):
        self.assertEqual(self.exercise('import tty; tty.setraw(1)'), (0, False))

    def test_single_echo_change_is_rejected(self):
        self.assertEqual(self.exercise('import termios; mode=termios.tcgetattr(1); mode[3]&=~termios.ECHO; termios.tcsetattr(1,termios.TCSANOW,mode)'), (0, False))

    def test_piped_stdin_reaches_eof(self):
        def write(child, master):
            child.stdin.write(b'context\n')
            child.stdin.close()
        self.assertEqual(self.exercise('import sys; data=sys.stdin.buffer.read(); raise SystemExit(0 if data==b"context\\n" else 9)',write,True), (0, True))

    def test_signals_reach_application_and_preserve_keeper(self):
        for value in [signal.SIGTERM, signal.SIGWINCH]:
            def send(child, master):
                data=b'';deadline=time.monotonic()+3
                while b'READY' not in data and time.monotonic()<deadline:
                    if select.select([master],[],[],.05)[0]:data+=os.read(master,4096)
                self.assertIn(b'READY',data)
                child.send_signal(value)
            source=f'import signal,sys,time; signal.signal({int(value)},lambda *_:sys.exit(7)); print("READY",flush=True); time.sleep(20)'
            self.assertEqual(self.exercise(source,send), (7, True))

    def test_hung_session_cleanup_is_bounded(self):
        def hung(child, master):
            with self.assertRaises(subprocess.TimeoutExpired):child.wait(timeout=.05)
        started=time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            self.exercise('import time; time.sleep(20)',hung)
        self.assertLess(time.monotonic()-started,8)


if __name__ == '__main__':
    unittest.main()
