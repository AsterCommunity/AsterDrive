"""Exercise the real controlling terminal used by rpassword, including echo restoration."""
import errno
import os
import pty
import select
import signal
import sys
import termios
import time

binary, database_url, working_dir = sys.argv[1:]
for args, prompt, succeeds in [
    (["config", "--database-url", database_url, "set", "--key", "mail_smtp_password"], b"Configuration value: ", True),
    (["node", "enroll", "--master-url", "https://example.invalid"], b"Enrollment token: ", False),
]:
    pid, terminal = pty.fork()
    if pid == 0:
        os.chdir(working_dir)
        os.execv(binary, [binary, *args])
    secret = b"pty-hidden-credential-marker"
    output = bytearray()
    sent = False
    exited = False
    status = None
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            if select.select([terminal], [], [], 0.02)[0]:
                try:
                    chunk = os.read(terminal, 8192)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                output.extend(chunk)
            # The library writes its prompt before disabling ECHO. Synchronize
            # with the terminal state, rather than racing a simulated typist.
            if prompt in output and not sent and not termios.tcgetattr(terminal)[3] & termios.ECHO:
                os.write(terminal, secret + b"\n")
                sent = True
            child, status = os.waitpid(pid, os.WNOHANG)
            if child:
                exited = True
                break
        if not exited:
            child, status = os.waitpid(pid, os.WNOHANG)
            exited = bool(child)
        assert sent, "hidden prompt never became ready: " + repr(output)
        assert exited, "interactive CLI did not terminate"
        assert secret not in output, "credential was echoed"
        assert bool(termios.tcgetattr(terminal)[3] & termios.ECHO), "terminal echo was not restored"
        assert (os.waitstatus_to_exitcode(status) == 0) == succeeds, repr(output)
    finally:
        if not exited:
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            except ProcessLookupError:
                pass
        os.close(terminal)
print("hidden terminal input and echo restoration passed for config and node")
