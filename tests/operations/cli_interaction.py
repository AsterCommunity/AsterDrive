"""Exercise inquire prompts and credential boundaries against a real controlling TTY."""
import errno
import fcntl
import json
import os
import pty
import re
import select
import signal
import subprocess
import struct
import sys
import termios
import time

binary, database_url, working_dir = sys.argv[1:]
environment = {key: value for key, value in os.environ.items()
               if key not in ("ASTER_CLI_CONFIG_KEY", "ASTER_CLI_CONFIG_VALUE",
                              "ASTER_CLI_MASTER_URL", "ASTER_CLI_ENROLLMENT_TOKEN", "CLICOLOR_FORCE")}
environment.update({"NO_COLOR": "1", "TERM": "xterm-256color"})
ansi = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")


def cli(*args):
    result = subprocess.run([binary, "config", "--database-url", database_url, *args],
                            cwd=working_dir, env=environment, capture_output=True, timeout=20)
    assert result.returncode == 0, "fixture config command failed"
    return json.loads(result.stdout)


def saved(key):
    backup = cli("export", "--include-sensitive")
    return next(entry for entry in backup["configs"] if entry["key"] == key)


def interact(args, steps, succeeds=True, hidden=False, forbidden=(), root="config", extra_env=None,
             canceled=False, expect_color=False):
    # stdout is deliberately a pipe: prompts and terminal echo must not corrupt JSON.
    stdout_read, stdout_write = os.pipe()
    pid, terminal = pty.fork()
    if pid == 0:
        os.close(stdout_read)
        os.dup2(stdout_write, 1)
        os.close(stdout_write)
        os.chdir(working_dir)
        prefix = [binary, "config", "--database-url", database_url, "--output-format", "json"]
        if root == "node":
            prefix = [binary, "node", "--output-format", "json"]
        elif root != "config":
            prefix = [binary, root]
        child_environment = environment.copy()
        for key, value in (extra_env or {}).items():
            if value is None:
                child_environment.pop(key, None)
            else:
                child_environment[key] = value
        os.execve(binary, [*prefix, *args], child_environment)
    os.close(stdout_write)
    fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    output = bytearray()
    stdout = bytearray()
    exited = False
    step = 0
    offset = 0
    status = None
    cursor_reports = 0
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            for descriptor in select.select([terminal, stdout_read], [], [], 0.02)[0]:
                try:
                    chunk = os.read(descriptor, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                (output if descriptor == terminal else stdout).extend(chunk)
            requests = output.count(b"\x1b[6n")
            if requests > cursor_reports:
                os.write(terminal, b"\x1b[1;1R" * (requests - cursor_reports))
                cursor_reports = requests
            if step < len(steps):
                prompt, response, expected_echo = steps[step]
                position = ansi.sub(b"", output).find(prompt, offset)
                echo = bool(termios.tcgetattr(terminal)[3] & termios.ECHO)
                if position >= 0 and echo == expected_echo:
                    if root == "node" and step == 0 and prompt == b"Primary URL":
                        assert b"Enter a non-empty" not in output, "validation appeared before input"
                    offset = position + len(prompt)
                    os.write(terminal, response)
                    step += 1
            child, status = os.waitpid(pid, os.WNOHANG)
            if child:
                exited = True
                break
        assert exited, f"interactive CLI timed out after {step}/{len(steps)} inputs"
        # Drain data queued just before process exit.
        while select.select([terminal, stdout_read], [], [], 0)[0]:
            read_any = False
            for descriptor in select.select([terminal, stdout_read], [], [], 0)[0]:
                try:
                    chunk = os.read(descriptor, 65536)
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
                    chunk = b""
                if chunk:
                    read_any = True
                    (output if descriptor == terminal else stdout).extend(chunk)
            if not read_any:
                break
        assert step == len(steps), f"interactive CLI missed expected prompts: {step}/{len(steps)}"
        assert bool(termios.tcgetattr(terminal)[3] & termios.ECHO), "terminal echo was not restored"
        assert (os.waitstatus_to_exitcode(status) == 0) == succeeds, "unexpected CLI exit status"
        assert bool(re.search(rb"\x1b\[[0-9;]*m", output)) == expect_color, "unexpected terminal color policy"
        if canceled:
            assert os.waitstatus_to_exitcode(status) == 130, "cancellation did not use exit status 130"
            assert b"Canceled." in output, "missing friendly cancellation notice"
            assert b"Validation Error" not in output, "cancellation was reported as invalid input"
            plain = ansi.sub(b"", output)
            assert re.search(rb"\r?\nCanceled\.\r?\n", plain), "cancellation overwrote the prompt line"
            assert plain.splitlines()[-1] == b"Canceled.", "prompt text remained after cancellation notice"
        for marker in forbidden:
            assert marker not in output and marker not in stdout, "sensitive value leaked"
        if succeeds:
            if root not in ("config", "node"):
                assert stdout.startswith(b"$argon2id$") and stdout.count(b"\n") == 1
                return bytes(stdout), ansi.sub(b"", output)
            result = json.loads(stdout)
            assert result["ok"] is True
            if hidden:
                assert b"Current value: [hidden sensitive value]" in output
            return result, ansi.sub(b"", output)
        assert not stdout, "canceled command wrote a success result"
        return None, output
    finally:
        if not exited:
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            except ProcessLookupError:
                pass
        os.close(terminal)
        os.close(stdout_read)


normal_prompt = b"New value "
secret_prompt = b"New value (masked) "
old_secret = b"old-sensitive-value-marker"
new_secret = b"new-sensitive-value-marker"
cli("set", "branding_title", "Original title")
cli("set", "mail_smtp_password", old_secret.decode())
private_backup = os.path.join(working_dir, "custom-sensitive.json")
with os.fdopen(os.open(private_backup, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as stream:
    json.dump({"format": "asterdrive.runtime-config", "version": 1,
               "configs": [{"key": "custom.sensitive", "value": old_secret.decode(),
                            "source": "custom", "visibility": "private", "is_sensitive": True}],
               "omitted_sensitive_keys": []}, stream)
cli("import", "--input-file", private_backup)

result, output = interact(["set", "branding_title"],
                          [(normal_prompt, b"Visible title marker", False),
                           (b"Visible title marker", b"\r", False)])
assert b"Visible title marker" in output, "ordinary input was not echoed"
assert result["data"]["value"] == "Visible title marker"
assert saved("branding_title")["value"] == "Visible title marker"

result, output = interact(["set", "branding_title", "--value-prompt"],
                          [(normal_prompt, b"Explicit visible prompt\r", False)])
assert b"Explicit visible prompt" in output

result, _ = interact(["set", "mail_smtp_password"],
                     [(secret_prompt, new_secret, False),
                      (b"*" * len(new_secret), b"\r", False)], hidden=True,
                     forbidden=(old_secret, new_secret))
assert saved("mail_smtp_password")["value"] == new_secret.decode()
# Attempt the library's optional reveal shortcut while editing, then cancel.
# It must never reveal the buffer and must not change the saved credential.
interact(["set", "mail_smtp_password"],
         [(secret_prompt, new_secret + b"\x12\x03", False)], succeeds=False, canceled=True,
         forbidden=(old_secret, new_secret))
assert saved("mail_smtp_password")["value"] == new_secret.decode()
interact(["set", "mail_smtp_password", "--value-prompt"],
         [(secret_prompt, old_secret + b"\r", False)], hidden=True,
         forbidden=(old_secret, new_secret))
interact(["set", "custom.sensitive"],
         [(secret_prompt, new_secret + b"\r", False)], hidden=True,
         forbidden=(old_secret, new_secret))
assert saved("custom.sensitive")["value"] == new_secret.decode()

for key, prompt, echo in [("branding_title", normal_prompt, False),
                          ("mail_smtp_password", secret_prompt, False),
                          ("custom.sensitive", secret_prompt, False)]:
    before = cli("get", key)
    interact(["set", key], [(prompt, b"\r", echo)], forbidden=(old_secret, new_secret))
    assert cli("get", key) == before, "Enter changed value or timestamp"
    interact(["set", key], [(prompt, b"\x03", echo)], succeeds=False,
             forbidden=(old_secret, new_secret), canceled=True)
    assert cli("get", key) == before, "Ctrl+C changed value or timestamp"
    interact(["set", key], [(prompt, b"\x04", echo)], succeeds=False,
             forbidden=(old_secret, new_secret), canceled=True)
    assert cli("get", key) == before, "EOF changed value or timestamp"
    interact(["set", key], [(prompt, b"\x1b", echo)], succeeds=False,
             forbidden=(old_secret, new_secret), canceled=True)
    assert cli("get", key) == before, "Escape changed value or timestamp"

result, output = interact(["set", "auth_cookie_secure"],
    [(normal_prompt, b"not-a-boolean\r", False),
     (b"configuration validation failed", b"\x7f" * len(b"not-a-boolean") + b"2\r", False)])
assert result["data"]["value"] == "false"

result, _ = interact(["set"],
    [(b"Configuration key / search", b"branding_title\r", False),
     (normal_prompt, b"Selected title\r", False)], forbidden=(old_secret, new_secret))
assert result["data"]["key"] == "branding_title"

result, _ = interact(["set"],
    [(b"Configuration key / search", b"Public browser title\r", False),
     (normal_prompt, b"Search result title\r", False)], forbidden=(old_secret, new_secret))
assert result["data"]["key"] == "branding_title"
assert saved("branding_title")["value"] == "Search result title"

# Match the numeric-setting interaction, with stdout redirected but stderr on
# the TTY. Its metadata and cancellation notice must retain terminal colors.
_, output = interact(["set", "wopi_discovery_cache_ttl_secs"],
    [(normal_prompt, b"\x03", False)], succeeds=False, canceled=True,
    extra_env={"NO_COLOR": None}, expect_color=True)
assert re.search(rb"\x1b\[[0-9;]+mwopi_discovery_cache_ttl_secs", output)
assert re.search(rb"\x1b\[[0-9;]+mType:", output)

# Enrollment prompts show the primary URL and hide tokens. A primary-mode
# fixture rejects enrollment before provider I/O or database writes.
with open(os.path.join(working_dir, "data/config.toml"), "rb") as stream:
    initial_config = stream.read()
interact(["enroll"], [(b"Primary URL", b"\x03", False)],
         succeeds=False, root="node", canceled=True)
with open(os.path.join(working_dir, "data/config.toml"), "rb") as stream:
    assert stream.read() == initial_config, "canceling the URL prompt changed instance configuration"
_, output = interact(["enroll"],
    [(b"Primary URL", b"bad\r", False),
     (b"Enter a non-empty", b"\x7f" * 3 + b"https://example.invalid\r", False),
     (b"Enrollment token", b"\r", False),
     (b"enrollment token must be non-empty", new_secret, False),
     (b"*" * len(new_secret), b"\r", False)],
    succeeds=False, root="node", forbidden=(new_secret,))
assert b"https://example.invalid" in output
assert b"start_mode" in output
interact(["enroll", "https://example.invalid", "--token-prompt"],
    [(b"Enrollment token", b"\x03", False)], succeeds=False, root="node",
    extra_env={"ASTER_CLI_ENROLLMENT_TOKEN": old_secret.decode()}, forbidden=(old_secret,), canceled=True)
interact(["enroll", "https://example.invalid"],
    [(b"Enrollment token", b"\x04", False)], succeeds=False, root="node", canceled=True)

interact([], [(b"Password (masked)", new_secret, False),
              (b"*" * len(new_secret), b"\r", False)],
         root="hash-password", forbidden=(new_secret,))
interact([], [(b"Password (masked)", b"\r", False),
              (b"must contain 1..4096", new_secret + b"\r", False)],
         root="hash-password", forbidden=(new_secret,))
for key in (b"\x03", b"\x04", b"\x1b"):
    interact([], [(b"Password (masked)", key, False)],
             root="hash-password", succeeds=False, canceled=True)
print("CLI visibility, retries, keep, cancel, EOF, JSON, selection, enrollment and hashing passed")
