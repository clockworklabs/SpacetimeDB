#!/usr/bin/env python

import subprocess
import unittest
import argparse
import os
import re
import fnmatch
import json
from . import TEST_DIR, SPACETIME_BIN, BASE_STDB_CONFIG_PATH, exe_suffix, build_template_target
import smoketests
import sys
import logging
import itertools
import tempfile
from pathlib import Path
import shutil

def check_docker():
    docker_ps = smoketests.run_cmd("docker", "ps", "--format=json")
    docker_ps = (json.loads(line) for line in docker_ps.splitlines())
    for docker_container in docker_ps:
        if "node" in docker_container["Image"] or "spacetime" in docker_container["Image"]:
            return docker_container["Names"]
    else:
        print("Docker container not found, is SpacetimeDB running?")
        exit(1)

def check_dotnet() -> bool:
    try:
        version = smoketests.run_cmd("dotnet", "--version", log=False).strip()
        if int(version.split(".")[0]) < 8:
            logging.info(f"dotnet version {version} not high enough (< 8.0), skipping dotnet smoketests")
            return False
    except Exception:
        return False
    return True

class ExclusionaryTestLoader(unittest.TestLoader):
    def __init__(self, excludelist=()):
        super().__init__()
        # build a regex that matches any of the elements of excludelist at a word boundary
        excludes = '|'.join(fnmatch.translate(exclude).removesuffix(r"\Z") for exclude in excludelist)
        self.excludepat = excludes and re.compile(rf"^(?:{excludes})\b")

    def loadTestsFromName(self, name, module=None):
        if self.excludepat:
            qualname = name
            if module is not None:
                qualname = module.__name__ + "." + name
            if self.excludepat.match(qualname):
                return self.suiteClass([])
        return super().loadTestsFromName(name, module)

def _convert_select_pattern(pattern):
    return f'*{pattern}*' if '*' not in pattern else pattern


TESTPREFIX = "smoketests.tests."
def main():
    tests = [fname.removesuffix(".py") for fname in os.listdir(TEST_DIR / "tests") if fname.endswith(".py") and fname != "__init__.py"]

    parser = argparse.ArgumentParser()
    parser.add_argument("test", nargs="*", default=tests)
    parser.add_argument("--docker", action="store_true")
    parser.add_argument("--compose-file")
    parser.add_argument("--no-docker-logs", action="store_true")
    parser.add_argument("--skip-dotnet", action="store_true", help="ignore tests which require dotnet")
    parser.add_argument("--show-all-output", action="store_true", help="show all stdout/stderr from the tests as they're running")
    parser.add_argument("--parallel", action="store_true", help="run test classes in parallel")
    parser.add_argument("-j", dest='jobs', help="Set number of jobs for parallel test runs. Default is `nproc`", type=int, default=0)
    parser.add_argument('-k', dest='testNamePatterns',
                        action='append', type=_convert_select_pattern,
                        help='Only run tests which match the given substring')
    parser.add_argument("-x", dest="exclude", nargs="*", default=[])
    parser.add_argument("--no-build-cli", action="store_true", help="don't cargo build the cli")
    parser.add_argument("--list", action="store_true", help="list the tests that would be run, but don't run them")
    parser.add_argument("--remote-server", action="store", help="Run against a remote server")
    parser.add_argument("--spacetime-login", action="store_true", help="Use `spacetime login` for these tests (and disable tests that don't work with that)")
    args = parser.parse_args()

    if args.docker:
        # have docker logs print concurrently with the test output
        if args.compose_file:
            smoketests.COMPOSE_FILE = args.compose_file
        if not args.no_docker_logs:
            if args.compose_file:
                subprocess.Popen(["docker", "compose", "-f", args.compose_file, "logs", "-f"])
            else:
                docker_container = check_docker()
                subprocess.Popen(["docker", "logs", "-f", docker_container])
        smoketests.HAVE_DOCKER = True

    if not args.skip_dotnet:
        smoketests.HAVE_DOTNET = check_dotnet()
        if not smoketests.HAVE_DOTNET:
            print("no suitable dotnet installation found")
            exit(1)

    add_prefix = lambda testlist: [TESTPREFIX + test for test in testlist]
    import fnmatch
    excludelist = add_prefix(args.exclude)
    testlist = add_prefix(args.test)

    loader = ExclusionaryTestLoader(excludelist)
    loader.testNamePatterns = args.testNamePatterns

    tests = loader.loadTestsFromNames(testlist)
    if args.list:
        print("Selected tests:\n")
        for test in itertools.chain(*itertools.chain(*tests)):
            print(f"{test}")
        exit(0)

    if not args.no_build_cli:
        logging.info("Compiling spacetime cli...")
        smoketests.run_cmd("cargo", "build", cwd=TEST_DIR.parent, capture_stderr=False)

    update_bin_name = "spacetimedb-update" + exe_suffix
    try:
        bin_is_symlink = SPACETIME_BIN.readlink() == update_bin_name
    except OSError:
        bin_is_symlink = False
    if not bin_is_symlink:
        try:
            os.remove(SPACETIME_BIN)
        except FileNotFoundError:
            pass
        try:
            os.symlink(update_bin_name, SPACETIME_BIN)
        except OSError:
            shutil.copyfile(SPACETIME_BIN.with_name(update_bin_name), SPACETIME_BIN)

    os.environ["SPACETIME_SKIP_CLIPPY"] = "1"

    with tempfile.NamedTemporaryFile(mode="w+b", suffix=".toml", buffering=0, delete_on_close=False) as config_file:
        with BASE_STDB_CONFIG_PATH.open("rb") as src, config_file.file as dst:
            shutil.copyfileobj(src, dst)

        if args.remote_server is not None:
            smoketests.spacetime("--config-path", config_file.name, "server", "edit", "localhost", "--url", args.remote_server, "--yes")
            smoketests.REMOTE_SERVER = True

        if args.spacetime_login:
            smoketests.spacetime("--config-path", config_file.name, "logout")
            smoketests.spacetime("--config-path", config_file.name, "login")
            smoketests.USE_SPACETIME_LOGIN = True
        else:
            smoketests.new_identity(config_file.name)

        smoketests.STDB_CONFIG = Path(config_file.name).read_text()

    build_template_target()
    buffer = not args.show_all_output
    verbosity = 2

    if args.parallel:
        print("parallel test running is under construction, this will probably not work correctly")
        from . import unittest_parallel
        unittest_parallel.main(buffer=buffer, verbose=verbosity, level="class", discovered_tests=tests, jobs=args.jobs)
    else:
        result = unittest.TextTestRunner(buffer=buffer, verbosity=verbosity).run(tests)
        if not result.wasSuccessful():
            parser.exit(status=1)


if __name__ == '__main__':
    main()
