"""Test hash input on a controlling PTY, capturing both process outputs separately."""
import errno
import json
import os
import pty
import select
import signal
import sys
import tempfile
import termios
import time

binary, working_dir = sys.argv[1:]
cases = json.load(sys.stdin)
hashes = []
for index, case in enumerate(cases):
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        ready_read, ready_write = os.pipe()
        pid, terminal = pty.fork()
        if pid == 0:
            os.close(ready_write)
            os.read(ready_read, 1)
            os.close(ready_read)
            os.chdir(working_dir)
            os.dup2(stdout.fileno(), 1)
            os.dup2(stderr.fileno(), 2)
            os.execv(binary, [binary, "hash-password"])
        os.close(ready_read)
        original_mode = termios.tcgetattr(terminal)
        os.set_blocking(terminal, False)
        os.write(ready_write, b"1")
        os.close(ready_write)
        output = bytearray()
        pending = None
        sent = False
        exited = False
        status = None
        try:
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                readable, writable, _ = select.select(
                    [terminal], [terminal] if pending else [], [], 0.02
                )
                if readable:
                    try:
                        chunk = os.read(terminal, 8192)
                    except BlockingIOError:
                        chunk = b""
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
                        chunk = b""
                    output.extend(chunk)
                if b"Password: " in output and pending is None and not termios.tcgetattr(terminal)[3] & termios.ECHO:
                    pending = case["input"].encode("utf-8")
                    sent = not pending
                if writable and pending:
                    try:
                        written = os.write(terminal, pending[:1024])
                    except BlockingIOError:
                        written = 0
                    pending = pending[written:]
                    sent = not pending
                child, status = os.waitpid(pid, os.WNOHANG)
                if child:
                    exited = True
                    break
            assert pending is not None, f"case {index}: hidden prompt never became ready"
            assert sent, f"case {index}: interactive CLI did not consume test input before deadline"
            assert exited, f"case {index}: interactive CLI timed out"
            # Drain terminal output after exit; no credential may leak during restoration.
            while select.select([terminal], [], [], 0)[0]:
                try:
                    chunk = os.read(terminal, 8192)
                except BlockingIOError:
                    break
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    break
                if not chunk:
                    break
                output.extend(chunk)
            assert output == b"Password: \r\n", f"case {index}: terminal output must contain only prompt and line ending"
            assert termios.tcgetattr(terminal) == original_mode, f"case {index}: terminal mode not restored"
            stdout.seek(0)
            stderr.seek(0)
            result, errors = stdout.read(), stderr.read()
            assert b"pty-hidden-credential-marker" not in errors, f"case {index}: credential leaked to stderr"
            succeeded = os.waitstatus_to_exitcode(status) == 0
            assert succeeded == case["succeeds"], f"case {index}: unexpected exit state"
            if succeeded:
                assert result.startswith(b"$argon2id$") and len(result.splitlines()) == 1, f"case {index}: stdout must contain one hash"
                assert not errors, f"case {index}: success stderr must be empty"
                hashes.append(result.decode("utf-8"))
            else:
                assert not result, f"case {index}: failure stdout must be empty"
                hashes.append(None)
        finally:
            if not exited:
                try:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                except ProcessLookupError:
                    pass
            os.close(terminal)
json.dump(hashes, sys.stdout)
