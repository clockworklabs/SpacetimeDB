from pathlib import Path
import contextlib
import json
import os
import random
import re
import shutil
import string
import string
import subprocess
import sys
import tempfile
import threading
import unittest
import logging
import http.client
import tomllib

# miscellaneous file paths
TEST_DIR = Path(__file__).parent
STDB_DIR = TEST_DIR.parent
exe_suffix = ".exe" if sys.platform == "win32" else ""
SPACETIME_BIN = STDB_DIR / ("target/debug/spacetime" + exe_suffix)
TEMPLATE_TARGET_DIR = STDB_DIR / "target/_stdbsmoketests"
STDB_CONFIG = TEST_DIR / "config.toml"

# the contents of files for the base smoketest project template
TEMPLATE_LIB_RS = open(STDB_DIR / "crates/cli/src/subcommands/project/rust/lib._rs").read()
TEMPLATE_CARGO_TOML = open(STDB_DIR / "crates/cli/src/subcommands/project/rust/Cargo._toml").read()
bindings_path = (STDB_DIR / "crates/bindings").absolute()
escaped_bindings_path = str(bindings_path).replace('\\', '\\\\\\\\') # double escape for re.sub + toml
TEMPLATE_CARGO_TOML = (re.compile(r"^spacetimedb\s*=.*$", re.M) \
    .sub(f'spacetimedb = {{ path = "{escaped_bindings_path}", features = {{features}} }}', TEMPLATE_CARGO_TOML))

# this is set to true when the --docker flag is passed to the cli
HAVE_DOCKER = False
# this is set to true when the --skip-dotnet flag is not passed to the cli,
# and a dotnet installation is detected
HAVE_DOTNET = False

# we need to late-bind the output stream to allow unittests to capture stdout/stderr.
class CapturableHandler(logging.StreamHandler):

    @property
    def stream(self):
        return sys.stderr

    @stream.setter
    def stream(self, value):
        pass

handler = CapturableHandler()
handler.setFormatter(logging.Formatter("%(asctime)s - %(levelname)s - %(message)s"))
logging.getLogger().addHandler(handler)
logging.getLogger().setLevel(logging.DEBUG)

def requires_dotnet(item):
    if HAVE_DOTNET:
        return item
    return unittest.skip("dotnet 8.0 not available")(item)


def build_template_target():
    if not TEMPLATE_TARGET_DIR.exists():
        logging.info("Building base compilation artifacts")
        class BuildModule(Smoketest):
            AUTOPUBLISH = False

        BuildModule.setUpClass()
        env = { **os.environ, "CARGO_TARGET_DIR": str(TEMPLATE_TARGET_DIR) }
        spacetime("build", "--project-path", BuildModule.project_path, env=env)
        BuildModule.tearDownClass()
        BuildModule.doClassCleanups()


def requires_docker(item):
    if HAVE_DOCKER:
        return item
    return unittest.skip("docker not available")(item)

def random_string(k=20):
    return ''.join(random.choices(string.ascii_lowercase, k=k))

def extract_fields(cmd_output, field_name):
    """
    parses output from the spacetime cli that's formatted in the "empty" style
    from tabled:
        FIELDNAME1    VALUE1
        THEFIELDNAME2 VALUE2
    field_name should be which field name you want to filter for
    """
    out = []
    for line in cmd_output.splitlines():
        fields = line.split()
        if len(fields) < 2:
            continue
        label, val, *_ = fields
        if label == field_name:
            out.append(val)
    return out

def extract_field(cmd_output, field_name):
    field, = extract_fields(cmd_output, field_name)
    return field

def log_cmd(args):
    logging.debug(f"$ {' '.join(str(arg) for arg in args)}")


def run_cmd(*args, capture_stderr=True, check=True, full_output=False, cmd_name=None, log=True, **kwargs):
    if log:
        log_cmd(args if cmd_name is None else [cmd_name, *args[1:]])

    needs_close = False
    if not capture_stderr:
        logging.debug(f"--- stderr ---")
        needs_close = True

    output = subprocess.run(
        list(args),
        encoding="utf8",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE if capture_stderr else None,
        **kwargs
    )
    if log:
        if capture_stderr and output.stderr.strip() != "":
            logging.debug(f"--- stderr ---\n{output.stderr.strip()}")
            needs_close = True
        if output.stdout.strip() != "":
            logging.debug(f"--- stdout ---\n{output.stdout.strip()}")
            needs_close = True
        if needs_close:
            logging.debug("--------------\n")

        sys.stderr.flush()
    if check:
        if cmd_name is not None:
            output.args[0] = cmd_name
        output.check_returncode()
    return output if full_output else output.stdout

def spacetime(*args, **kwargs):
    return run_cmd(SPACETIME_BIN, *args, cmd_name="spacetime", **kwargs)

def new_identity(config_path):
    spacetime("--config-path", str(config_path), "logout")
    spacetime("--config-path", str(config_path), "login", "--server-issued-login", "localhost", full_output=False)

class Smoketest(unittest.TestCase):
    MODULE_CODE = TEMPLATE_LIB_RS
    AUTOPUBLISH = True
    BINDINGS_FEATURES = []
    EXTRA_DEPS = ""

    @classmethod
    def cargo_manifest(cls, manifest_text):
        return manifest_text.replace("{features}", repr(list(cls.BINDINGS_FEATURES))) + cls.EXTRA_DEPS

    # helpers

    @classmethod
    def spacetime(cls, *args, **kwargs):
        return spacetime("--config-path", str(cls.config_path), *args, **kwargs)

    def _check_published(self):
        if not hasattr(self, "database_identity"):
            raise Exception("Cannot use this function without publishing a module")

    def call(self, reducer, *args, anon=False):
        self._check_published()
        anon = ["--anonymous"] if anon else []
        self.spacetime("call", *anon, "--", self.database_identity, reducer, *map(json.dumps, args))

    def logs(self, n):
        return [log["message"] for log in self.log_records(n)]

    def log_records(self, n):
        self._check_published()
        logs = self.spacetime("logs", "--format=json", "-n", str(n), "--", self.database_identity)
        return list(map(json.loads, logs.splitlines()))

    def publish_module(self, domain=None, *, clear=True, capture_stderr=True):
        publish_output = self.spacetime(
            "publish",
            *[domain] if domain is not None else [],
            *["-c"] if clear and domain is not None else [],
            "--project-path", self.project_path,
            # This is required if -c is provided, but is also required for SpacetimeDBPrivate's tests,
            # because the server address is `node` which doesn't look like `localhost` or `127.0.0.1`
            # and so the publish step prompts for confirmation.
            "--yes",
            capture_stderr=capture_stderr,
        )
        self.resolved_identity = re.search(r"identity: ([0-9a-fA-F]+)", publish_output)[1]
        self.database_identity = domain if domain is not None else self.resolved_identity

    @classmethod
    def reset_config(cls):
        shutil.copy(STDB_CONFIG, cls.config_path)

    def fingerprint(self):
        # Fetch the server's fingerprint; required for `identity list`.
        self.spacetime("server", "fingerprint", "localhost", "-y")

    def new_identity(self):
        new_identity(self.__class__.config_path)

    def subscribe(self, *queries, n):
        self._check_published()
        assert isinstance(n, int)

        args = [SPACETIME_BIN, "--config-path", str(self.config_path),"subscribe", self.database_identity, "-t", "60", "-n", str(n), "--print-initial-update", "--", *queries]
        fake_args = ["spacetime", *args[1:]]
        log_cmd(fake_args)

        proc = subprocess.Popen(args, encoding="utf8", stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        def stderr_task():
            sys.stderr.writelines(proc.stderr)
        threading.Thread(target=stderr_task).start()

        init_update = proc.stdout.readline().strip()
        if init_update:
            print("initial update:", init_update)
        else:
            try:
                code = proc.wait()
                if code:
                    raise subprocess.CalledProcessError(code, fake_args)
                print("no inital update, but no error code either")
            except subprocess.TimeoutExpired:
                print("no initial update, but process is still running")

        def run():
            updates = list(map(json.loads, proc.stdout))
            code = proc.wait()
            if code:
                raise subprocess.CalledProcessError(code, fake_args)
            return updates
        # Note that we're returning `.join`, not `.join()`; this returns something that the caller can call in order to
        # join the thread and wait for the results.
        # If the caller does not invoke this returned value, the thread will just run in the background, not be awaited,
        # and **not raise any exceptions to the caller**.
        return ReturnThread(run).join

    def api_call(self, method, path, body = None, headers = {}):
        with open(self.config_path, "rb") as f:
            config = tomllib.load(f)
            host = config['default_server']
            token = config['spacetimedb_token']
            conn = http.client.HTTPConnection(host)
            auth = {"Authorization": f'Bearer {token}'}
            headers.update(auth)
            log_cmd([method, path])
            conn.request(method, path, body, headers)
            resp = conn.getresponse()
            logging.debug(f"{resp.status} {resp.read()}")
            if resp.status != 200:
                raise resp
            resp


    @classmethod
    def write_module_code(cls, module_code):
        open(cls.project_path / "src/lib.rs", "w").write(module_code)

    # testcase initialization

    @classmethod
    def setUpClass(cls):
        cls.project_path = Path(cls.enterClassContext(tempfile.TemporaryDirectory()))
        cls.config_path = cls.project_path / "config.toml"
        cls.reset_config()
        open(cls.project_path / "Cargo.toml", "w").write(cls.cargo_manifest(TEMPLATE_CARGO_TOML))
        shutil.copy2(STDB_DIR / "rust-toolchain.toml", cls.project_path)
        os.mkdir(cls.project_path / "src")
        cls.write_module_code(cls.MODULE_CODE)
        if TEMPLATE_TARGET_DIR.exists():
            shutil.copytree(TEMPLATE_TARGET_DIR, cls.project_path / "target")

        if cls.AUTOPUBLISH:
            logging.info(f"Compiling module for {cls.__qualname__}...")
            cls.publish_module(cls, capture_stderr=True) # capture stderr because otherwise it clutters the top-level test logs for some reason.

    def tearDown(self):
        # if this single test method published a database, clean it up now
        if "database_identity" in self.__dict__:
            try:
                # TODO: save the credentials in publish_module()
                self.spacetime("delete", self.database_identity)
            except Exception:
                pass

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, "database_identity"):
            try:
                # TODO: save the credentials in publish_module()
                cls.spacetime("delete", cls.database_identity)
            except Exception:
                pass

    if sys.version_info < (3, 11):
        # polyfill; python 3.11 defines this classmethod on TestCase
        @classmethod
        def enterClassContext(cls, cm):
            result = cm.__enter__()
            cls.addClassCleanup(cm.__exit__, None, None, None)
            return result


# This is a custom thread class that will propagate an exception to the caller of `.join()`.
# This is required because, by default, threads do not propagate exceptions to their callers,
# even callers who have called `join`.
class ReturnThread:
    def __init__(self, target):
        self._target = target
        self._exception = None
        self._thread = threading.Thread(target=self._task)
        self._thread.start()

    def _task(self):
        # Wrap self._target()` with an exception handler, so we can return the exception
        # to the caller of `join` below.
        try:
            self._result = self._target()
        except BaseException as e:
            self._exception = e
        finally:
            del self._target

    def join(self, timeout=None):
        self._thread.join(timeout)
        if self._exception is not None:
            raise self._exception
        return self._result

