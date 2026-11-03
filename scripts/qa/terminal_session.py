"""Keep the controlling PTY alive while its application's exit/modes are observed."""
import json
import os
import select
import signal
import subprocess
import sys
import time


class TerminalChild:
    def __init__(self, command, *, slave, stdin, stdout, stderr, cwd, env):
        status_read, status_write = os.pipe()
        release_read, release_write = os.pipe()
        try:
            self.supervisor = subprocess.Popen(
                [sys.executable, os.path.abspath(__file__), str(status_write),
                 str(release_read), str(slave), *command],
                stdin=stdin, stdout=stdout, stderr=stderr, cwd=cwd, env=env,
                pass_fds=(status_write, release_read, slave))
        except BaseException:
            for fd in (status_read, status_write, release_read, release_write):
                os.close(fd)
            raise
        os.close(status_write)
        os.close(release_read)
        self._status = status_read
        self._release = release_write
        self._buffer = b''
        self._pid = None
        self.returncode = None
        self.stdin = self.supervisor.stdin
        deadline = time.monotonic() + 8
        try:
            while self._pid is None:
                self._read(.05)
                if time.monotonic() >= deadline:
                    raise TimeoutError('Terminal application did not start')
        except BaseException:
            self.close()
            raise

    @property
    def pid(self):
        return self._pid

    def _read(self, timeout=0):
        if select.select([self._status], [], [], timeout)[0]:
            chunk = os.read(self._status, 4096)
            if not chunk:
                if self.returncode is None:
                    raise RuntimeError('Terminal supervisor ended without application status')
                return
            self._buffer += chunk
            while b'\n' in self._buffer:
                line, self._buffer = self._buffer.split(b'\n', 1)
                message = json.loads(line)
                if 'pid' in message:
                    self._pid = message['pid']
                if 'exit' in message:
                    self.returncode = message['exit']

    def poll(self):
        self._read()
        return self.returncode

    def wait(self, timeout=None):
        deadline = None if timeout is None else time.monotonic() + timeout
        while self.poll() is None:
            if deadline is not None and time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired('terminal application', timeout)
            self._read(.02)
        return self.returncode

    def send_signal(self, value):
        os.kill(self.pid, value)

    def kill_session(self):
        try:
            os.killpg(self.supervisor.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        if self.supervisor.poll() is None:
            self.supervisor.kill()

    def close(self):
        # Call only after reading the untouched terminal modes. Ending the
        # session leader earlier revokes the controlling PTY on Darwin.
        # Retire the whole owned group, including descendants of an exited
        # shell that deliberately ignore hangup, before releasing the keeper.
        self.kill_session()
        if self._release is not None:
            os.close(self._release)
            self._release = None
        self.supervisor.wait(timeout=2)
        if self._status is not None:
            os.close(self._status)
            self._status = None


def supervise():
    import fcntl
    import termios
    status, release, slave = map(int, sys.argv[1:4])
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    child = subprocess.Popen(sys.argv[4:])
    with os.fdopen(status, 'w') as stream:
        stream.write(json.dumps({'pid': child.pid}) + '\n')
        stream.flush()
        result = child.wait()
        stream.write(json.dumps({'exit': result}) + '\n')
        stream.flush()
        # No tcsetattr here: restoration must be the application's work.
        os.read(release, 1)
    os.close(release)


if __name__ == '__main__':
    supervise()
