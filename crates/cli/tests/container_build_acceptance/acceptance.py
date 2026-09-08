#!/usr/bin/env python3
"""Explicitly invoked real local-builder acceptance, never part of cargo test.

Only an explicitly verified Docker Desktop Unix socket is accepted. The CLI
uses a fixture-owned BuildKit Unix proxy and never opens Spacetime credentials.
Downloaded tools and retained OCI output live under a new private workspace.
"""
import argparse
import concurrent.futures
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import selectors
import shutil
import shlex
import signal
import socket
import subprocess
import tarfile
import threading
import time
import urllib.request
import uuid


LOCK = json.loads(Path(__file__).with_name("tool-lock.json").read_text())
LIMIT = 32 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def environment(root):
    return {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(root / "home"),
        "TMPDIR": str(root / "tmp"),
    }


def run(args, env, timeout=120, check=True):
    # No shell expansion, inherited proxy/daemon/server variables or credentials.
    child = subprocess.Popen(args, env=env, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        out, err = child.communicate(timeout=timeout)
    except BaseException:
        child.kill()
        child.communicate(timeout=10)
        raise
    require(len(out) <= LIMIT and len(err) <= LIMIT, "fixture command output exceeded bound")
    if check:
        require(child.returncode == 0,
                f"fixture command failed: {Path(args[0]).name} (status {child.returncode})")
    return child.returncode, out, err


def tools(root):
    require(platform.system() == "Darwin" and platform.machine() == "arm64",
            "the checked-in tool lock supports Darwin arm64")
    directory = root / "tools"
    directory.mkdir()
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    binaries = {}
    for name, pin in LOCK["tools"].items():
        print(f"Downloading and verifying pinned {name} {pin['version']}", flush=True)
        with opener.open(pin["url"], timeout=60) as response:
            data = response.read(LIMIT + 1)
        require(len(data) <= LIMIT, "tool archive exceeded bound")
        require(hashlib.sha256(data).hexdigest() == pin["sha256"], "tool archive checksum mismatch")
        archive = directory / (name + ".tar.gz")
        archive.write_bytes(data)
        with tarfile.open(archive) as contents:
            matches = [item for item in contents if item.name.removeprefix("./") == pin["member"]]
            require(len(matches) == 1 and matches[0].isreg(), "unexpected tool archive member")
            require(matches[0].size <= 128 * 1024 * 1024, "tool binary exceeded bound")
            binary = directory / name
            with contents.extractfile(matches[0]) as source:
                binary.write_bytes(source.read())
        binary.chmod(0o700)
        binaries[name] = binary
    return binaries


class UnixProxy:
    """Forward only to this fixture's Docker-published numeric-loopback port."""
    def __init__(self, path, port):
        self.path, self.port = path, port
        self.stop = threading.Event()
        self.slots = threading.BoundedSemaphore(32)
        self.pool = concurrent.futures.ThreadPoolExecutor(max_workers=32)
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(path))
        path.chmod(0o600)
        self.listener.listen(32)
        self.listener.settimeout(0.2)
        self.thread = threading.Thread(target=self.accept)
        self.thread.start()

    def accept(self):
        while not self.stop.is_set():
            try:
                client, _ = self.listener.accept()
            except TimeoutError:
                continue
            except OSError:
                break
            if self.slots.acquire(blocking=False):
                self.pool.submit(self.forward, client)
            else:
                client.close()

    def forward(self, client):
        try:
            with client, socket.create_connection(("127.0.0.1", self.port), timeout=5) as upstream:
                # Bounded socket writes and periodic stop checks; no filesystem
                # path or request header can select another destination.
                client.settimeout(1)
                upstream.settimeout(1)
                with selectors.DefaultSelector() as select:
                    select.register(client, selectors.EVENT_READ, upstream)
                    select.register(upstream, selectors.EVENT_READ, client)
                    while not self.stop.is_set():
                        for key, _ in select.select(0.2):
                            data = key.fileobj.recv(64 * 1024)
                            if not data:
                                return
                            key.data.sendall(data)
        except (OSError, TimeoutError):
            pass
        finally:
            self.slots.release()

    def close(self):
        self.stop.set()
        self.listener.close()
        self.thread.join(timeout=3)
        require(not self.thread.is_alive(), "BuildKit proxy did not stop accepting")
        self.pool.shutdown(wait=True)
        self.path.unlink()


class Builder:
    def __init__(self, root, docker_socket, binaries):
        self.root, self.binaries = root, binaries
        self.env = environment(root)
        docker = shutil.which("docker")
        require(docker is not None, "Docker executable is required")
        self.docker = [docker, "--host", "unix://" + str(docker_socket),
                       "--config", str(root / "docker-config")]
        self.name = "stdb-cli-build-" + uuid.uuid4().hex
        self.volume = self.name + "-cache"
        self.container = None
        self.run_attempted = False
        self.proxy = None
        self.volume_created = False

    def command(self, *args, timeout=120, check=True):
        return run(self.docker + list(args), self.env, timeout, check)

    def start(self):
        _, info, _ = self.command("info", "--format", "{{.Name}}|{{.OperatingSystem}}|{{.OSType}}")
        require(info.decode().strip() == "docker-desktop|Docker Desktop|linux",
                "explicit socket is not the expected local Docker Desktop fixture host")
        print("Verified explicit Docker Desktop socket; starting isolated BuildKit", flush=True)
        self.command("pull", "--platform", "linux/arm64", LOCK["images"]["buildkit"], timeout=300)
        _, raw, _ = self.command("image", "inspect", LOCK["images"]["buildkit"])
        inspected = json.loads(raw)[0]
        require(LOCK["images"]["buildkit"] in inspected["RepoDigests"], "pulled BuildKit digest mismatch")
        require(inspected["Architecture"] == "arm64", "BuildKit architecture mismatch")
        self.command("volume", "create", "--label", "spacetimedb.fixture=" + self.name, self.volume)
        self.volume_created = True
        # A timed-out CLI may have created the container before losing its
        # response. Retain the exact name/label cleanup key before dispatch.
        self.run_attempted = True
        _, identifier, _ = self.command(
            "run", "--detach", "--pull", "never", "--platform", "linux/arm64",
            "--name", self.name, "--label", "spacetimedb.fixture=" + self.name,
            "--privileged", "--cpus", "2", "--memory", "2g", "--memory-swap", "2g",
            "--log-driver", "json-file", "--log-opt", "max-size=4m", "--log-opt", "max-file=2",
            "--pids-limit", "256", "--mount", f"type=volume,src={self.volume},dst=/var/lib/buildkit",
            "--publish", "127.0.0.1::1234", LOCK["images"]["buildkit"],
            "--addr", "tcp://0.0.0.0:1234", "--oci-worker-snapshotter=native")
        self.container = identifier.decode().strip()
        _, raw, _ = self.command("inspect", self.container)
        owned = json.loads(raw)[0]
        require(owned["Config"]["Labels"]["spacetimedb.fixture"] == self.name, "container ownership mismatch")
        ports = owned["NetworkSettings"]["Ports"]["1234/tcp"]
        require(len(ports) == 1 and ports[0]["HostIp"] == "127.0.0.1", "BuildKit is not loopback bound")
        self.proxy = UnixProxy(self.root / "buildkit.sock", int(ports[0]["HostPort"]))
        deadline = time.monotonic() + 30
        while True:
            status, _, _ = run([str(self.binaries["buildctl"]), "--addr", self.endpoint,
                                "debug", "workers"], self.env, timeout=5, check=False)
            if status == 0:
                break
            require(time.monotonic() < deadline, "BuildKit fixture did not become ready")
            time.sleep(0.2)

    @property
    def endpoint(self):
        return "unix://" + str(self.root / "buildkit.sock")

    def close(self):
        errors = []
        if self.proxy:
            try:
                self.proxy.close()
            except Exception as error:
                errors.append(str(error))
        if self.run_attempted:
            try:
                # Inspect only the exact returned ID or generated name. A
                # failed/ambiguous inspect is not proof of absence. The label
                # must match before deletion, including after a lost run reply.
                _, raw, _ = self.command("container", "inspect", self.container or self.name)
                objects = json.loads(raw)
                require(len(objects) == 1, "ambiguous owned container lookup")
                owned = objects[0]
                require(owned["Name"] == "/" + self.name and
                        owned["Config"]["Labels"]["spacetimedb.fixture"] == self.name,
                        "container cleanup ownership mismatch")
                if self.container:
                    require(owned["Id"] == self.container, "container cleanup ID mismatch")
                # A successful synchronous removal is positive completion.
                self.command("rm", "--force", "--volumes", owned["Id"])
            except Exception as error:
                errors.append(str(error))
        if self.volume_created:
            try:
                self.command("volume", "rm", self.volume)
            except Exception as error:
                errors.append(str(error))
        require(not errors, "positive builder teardown failed: " + "; ".join(errors))


def config(project, image):
    project.mkdir()
    document = {
        "database": "local-builder-fixture",
        "container": {
            "image": image,
            "env_keys": ["RUNTIME_ONLY"],
            "resources": {"cpu_millicores": 1000, "memory_bytes": 536870912,
                          "scratch_bytes": 1073741824, "pids_max": 128},
        },
    }
    (project / "spacetime.json").write_text(json.dumps(document))


def verify_layout(layout, secret):
    metadata = json.loads((layout / "prepared.json").read_text())
    require(secret not in (layout / "prepared.json").read_bytes(), "build secret entered prepared metadata")
    objects = {}
    for item in metadata["objects"]:
        descriptor = item["descriptor"]
        path = Path(item["path"])
        require(not path.is_absolute() and ".." not in path.parts, "artifact path escaped layout")
        data = (layout / path).read_bytes()
        require(len(data) == descriptor["size"], "artifact size mismatch")
        require("sha256:" + hashlib.sha256(data).hexdigest() == descriptor["digest"], "artifact hash mismatch")
        require(secret not in data, "build secret entered retained image object")
        if item["kind"] == "layer" and data[:2] == b"\x1f\x8b":
            require(secret not in gzip.decompress(data), "build secret entered retained image layer")
        objects[descriptor["digest"]] = (item["kind"], data)
    manifest = json.loads(objects[metadata["manifest"]["digest"]][1])
    require(set(objects) == {metadata["manifest"]["digest"], manifest["config"]["digest"],
                             *(entry["digest"] for entry in manifest["layers"])},
            "prepared closure is not the exact executable manifest/config/layers")
    image = json.loads(objects[manifest["config"]["digest"]][1])
    require(image["os"] == "linux" and image["architecture"] == "arm64", "image platform mismatch")
    return metadata, image


def cases(root, cli, binaries, builder, deadline_test):
    secret = ("build-secret-" + uuid.uuid4().hex).encode()
    secret_file = root / "build-secret"
    secret_file.write_bytes(secret)
    secret_file.chmod(0o600)
    base = [str(cli), "--root-dir", str(root / "cli-root"), "--config-path", str(root / "unused-config"),
            "container", "build", "--platform", "linux/arm64", "--buildctl", str(binaries["buildctl"]),
            "--railpack", str(binaries["railpack"]), "--buildkit-host", builder.endpoint]

    def build(project, output, success=True, secrets=False):
        args = base + ["--project-path", str(project), "--out-dir", str(output)]
        if secrets:
            args += ["--build-secret", "BUILD_SENTINEL=" + str(secret_file)]
        status, out, err = run(args, environment(root), timeout=1200, check=False)
        require(secret not in out + err, "CLI leaked builder secret diagnostics")
        if (status == 0) != success:
            # Error output has been checked for the generated sentinel and the
            # fixture never supplies ordinary or registry credentials.
            raise RuntimeError("CLI build outcome mismatch: " + (out + err).decode(errors="replace")[:2048])
        return out + err

    project = root / "dockerfile-project"
    config(project, {"build": {"builder": "dockerfile", "context": "."}})
    (project / "Dockerfile").write_text(
        f"FROM {LOCK['images']['alpine']}\n"
        "RUN --mount=type=secret,id=BUILD_SENTINEL test -s /run/secrets/BUILD_SENTINEL && cat /run/secrets/BUILD_SENTINEL >&2\n"
        "WORKDIR /app\nUSER 1001:1001\nENV IMAGE_DEFAULT=retained\n"
        'ENTRYPOINT ["/bin/sh"]\nCMD ["-c", "echo dockerfile-ready"]\n')
    output = root / "dockerfile-output"
    print("Actual Dockerfile build with explicit secret mount", flush=True)
    build(project, output, secrets=True)
    metadata, image = verify_layout(output, secret)
    require(metadata["container"]["argv"] == ["/bin/sh", "-c", "echo dockerfile-ready"], "argv was not normalized")
    require(metadata["container"]["user"] == "1001:1001", "image user was lost")
    require(metadata["container"]["working_directory"] == "/app", "image working directory was lost")
    require("IMAGE_DEFAULT=retained" in image["config"]["Env"], "image environment defaults were lost")

    project = root / "railpack-project"
    config(project, {"build": {"builder": "railpack", "context": "."}})
    (project / "start.sh").write_text("#!/bin/sh\nprintf 'railpack-ready\\n'\n")
    (project / "start.sh").chmod(0o700)
    output = root / "railpack-output"
    print("Actual explicitly selected Railpack build", flush=True)
    build(project, output)
    verify_layout(output, secret)

    print("Failed secret-using build must not expose logs or publish output", flush=True)
    project = root / "failed-project"
    config(project, {"build": {"builder": "dockerfile", "context": "."}})
    (project / "Dockerfile").write_text(
        f"FROM {LOCK['images']['alpine']}\n"
        "RUN --mount=type=secret,id=BUILD_SENTINEL cat /run/secrets/BUILD_SENTINEL >&2; exit 23\n")
    output = root / "failed-output"
    build(project, output, success=False, secrets=True)
    require(not output.exists(), "failed build published output")

    print("Explicit unsupported Railpack detection cannot fall back to Dockerfile", flush=True)
    project = root / "unsupported-project"
    config(project, {"build": {"builder": "railpack", "context": "."}})
    (project / "Dockerfile").write_text('FROM scratch\nCMD ["would-be-wrong-builder"]\n')
    output = root / "unsupported-output"
    build(project, output, success=False)
    require(not output.exists(), "unsupported Railpack silently used another builder")

    print("Tampering with a retained OCI object must fail before output publication", flush=True)
    tampered = root / "tampered-layout"
    shutil.copytree(root / "dockerfile-output", tampered)
    metadata = json.loads((tampered / "prepared.json").read_text())
    target = next(item for item in metadata["objects"] if item["kind"] == "config")
    with (tampered / target["path"]).open("ab") as changed:
        changed.write(b" ")
    project = root / "tampered-project"
    config(project, {"oci_ref": "oci:" + str(tampered)})
    output = root / "tampered-output"
    build(project, output, success=False)
    require(not output.exists(), "tampered OCI image was accepted")

    print("A path created during the actual build must never be replaced", flush=True)
    project = root / "replacement-project"
    config(project, {"build": {"builder": "dockerfile", "context": "."}})
    (project / "Dockerfile").write_text(
        f"FROM {LOCK['images']['alpine']}\nRUN sleep 3\nCMD [\"/bin/true\"]\n")
    output = root / "replacement-output"
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        task = pool.submit(build, project, output, False)
        deadline = time.monotonic() + 30
        while not list(root.glob(".spacetime-image-*")):
            require(not task.done(), "build finished before concurrent replacement could be tested")
            require(time.monotonic() < deadline, "build never created its private workspace")
            time.sleep(0.01)
        output.mkdir()
        (output / "sentinel").write_text("must survive")
        task.result(timeout=60)
    require(list(output.iterdir()) == [output / "sentinel"], "existing output was replaced")
    require((output / "sentinel").read_text() == "must survive", "existing output content changed")

    print("SIGINT must cancel the real buildctl and positively reap it", flush=True)
    project = root / "cancel-project"
    config(project, {"build": {"builder": "dockerfile", "context": "."}})
    (project / "Dockerfile").write_text(
        f"FROM {LOCK['images']['alpine']}\nRUN sleep 120\nCMD [\"/bin/true\"]\n")
    output = root / "cancel-output"
    pid_file = root / "actual-buildctl.pid"
    wrapper = root / "record-buildctl"
    wrapper.write_text("#!/bin/sh\nprintf '%s\\n' \"$$\" > " + shlex.quote(str(pid_file)) +
                       "\nexec " + shlex.quote(str(binaries["buildctl"])) + ' "$@"\n')
    wrapper.chmod(0o700)
    args = list(base)
    args[args.index("--buildctl") + 1] = str(wrapper)
    args += ["--project-path", str(project), "--out-dir", str(output)]
    child = subprocess.Popen(args, env=environment(root), stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        deadline = time.monotonic() + 30
        while not pid_file.exists():
            require(child.poll() is None, "CLI exited before invoking real buildctl")
            require(time.monotonic() < deadline, "real buildctl did not start")
            time.sleep(0.01)
        tool_pid = int(pid_file.read_text())
        child.send_signal(signal.SIGINT)
        out, err = child.communicate(timeout=15)
        require(child.returncode != 0 and b"cancelled" in out + err, "CLI did not report cancellation")
        require(secret not in out + err, "cancelled CLI leaked a secret")
        try:
            os.kill(tool_pid, 0)
        except ProcessLookupError:
            pass
        else:
            raise RuntimeError("real buildctl PID still exists after acknowledged CLI cancellation")
        require(not output.exists(), "cancelled build published output")
    finally:
        if child.poll() is None:
            child.kill()
        child.communicate(timeout=10)
    require(not list(root.glob(".spacetime-image-*")), "completed CLI left builder workspaces behind")

    print("A short harness deadline must reap real buildctl before releasing its workspace", flush=True)
    pid_file.unlink()
    deadline_env = environment(root)
    deadline_env.update({
        "STDB_BUILDER_CONTEXT": str(project),
        "STDB_BUILDER_BUILDCTL": str(wrapper),
        "STDB_BUILDER_PID_FILE": str(pid_file),
        "STDB_BUILDER_WORKSPACE": str(root),
        "STDB_BUILDER_SOCKET": builder.endpoint,
    })
    run([str(deadline_test), "--ignored", "--exact",
         "actual_buildctl_deadline_reaps_before_workspace_release", "--test-threads=1"],
        deadline_env, timeout=30)
    return {
        "version": 1,
        "dockerfile": str(root / "dockerfile-output"),
        "railpack": str(root / "railpack-output"),
        "tool_lock": LOCK,
        "checks": ["actual_dockerfile", "actual_railpack", "secret_logs", "failed_build",
                   "no_fallback", "tampered_closure", "concurrent_output", "cancel_and_reap",
                   "deadline_and_reap"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docker-socket", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--deadline-test-binary", type=Path, required=True)
    parser.add_argument("--workspace", type=Path, required=True)
    args = parser.parse_args()
    require(args.docker_socket.is_absolute() and args.docker_socket.is_socket(), "explicit Docker Unix socket required")
    require(args.cli.is_absolute() and args.cli.is_file(), "absolute prebuilt CLI executable required")
    require(args.deadline_test_binary.is_absolute() and args.deadline_test_binary.is_file(),
            "absolute prebuilt deadline test executable required")
    require(args.workspace.is_absolute() and not args.workspace.exists(), "workspace must be a new absolute directory")
    args.workspace.mkdir(mode=0o700)
    for child in ["home", "tmp", "docker-config"]:
        (args.workspace / child).mkdir(mode=0o700)
    # A missing named config proves local build dispatch does not read global
    # credentials or open the supplied ordinary configuration path.
    binaries = tools(args.workspace)
    builder = Builder(args.workspace, args.docker_socket, binaries)
    try:
        builder.start()
        receipt = cases(args.workspace, args.cli, binaries, builder, args.deadline_test_binary)
    finally:
        builder.close()
    (args.workspace / "acceptance.json").write_text(json.dumps(receipt, indent=2))
    print("Builder acceptance passed; retained OCI layouts are ready for managed publication", flush=True)


if __name__ == "__main__":
    main()
