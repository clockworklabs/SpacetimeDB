#!/usr/bin/env python

import subprocess
import unittest
import argparse
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory
from pathlib import Path
import os
import re
import json
from . import TEST_DIR, STDB_DIR, STDB_CONFIG


print("Compiling spacetime cli...")
check_call(["cargo", "build"], cwd=TEST_DIR.parent)

os.environ["SPACETIME_SKIP_CLIPPY"] = "1"
os.environ["CARGO_TARGET_DIR"] = str(STDB_DIR / "target")

def check_docker():
    docker_ps = check_output(["docker", "ps", "--format=json"])
    docker_ps = (json.loads(line) for line in docker_ps.splitlines())
    for docker_container in docker_ps:
        if "node" in docker_container["Image"]:
            break
    else:
        print("Docker container not found, is SpacetimeDB running?")
        exit(1)

    check_call(["docker", "logs", docker_container["Names"]])

def main():
    tests = [fname.removesuffix(".py") for fname in os.listdir(TEST_DIR / "tests") if fname.endswith(".py") and fname != "__init__.py"]

    loader = unittest.TestLoader()
    parser = argparse.ArgumentParser()
    parser.add_argument("testfile", nargs="*", default=tests)
    parser.add_argument("--docker", action="store_true")
    parser.add_argument("--show-all-output", action="store_true", help="show all stdout/stderr from the tests as they're running")
    parser.add_argument("--parallel", action="store_true", help="run test classes in parallel")
    parser.add_argument("-x", "--exclude", nargs="*")
    args = parser.parse_args()

    if args.docker:
        check_docker()

    if args.exclude:
        for x in args.exclude:
            tests.remove(x)

    tests = loader.loadTestsFromNames("smoketests.tests." + test for test in args.testfile)
    buffer = not args.show_all_output
    verbosity = 2

    if args.parallel:
        from . import unittest_parallel
        unittest_parallel.main(buffer=buffer, verbose=verbosity, level="class", discovered_tests=tests)
    else:
        unittest.TextTestRunner(buffer=buffer, verbosity=verbosity).run(tests)


if __name__ == '__main__':
    main()
