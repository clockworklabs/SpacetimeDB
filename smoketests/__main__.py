#!/usr/bin/env python

import subprocess
import unittest
import argparse
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory
from pathlib import Path
import os
import re
import fnmatch
import json
from . import TEST_DIR, STDB_DIR, STDB_CONFIG, build_template_target
import smoketests


def check_docker():
    docker_ps = check_output(["docker", "ps", "--format=json"])
    docker_ps = (json.loads(line) for line in docker_ps.splitlines())
    for docker_container in docker_ps:
        if "node" in docker_container["Image"]:
            return docker_container["Names"]
    else:
        print("Docker container not found, is SpacetimeDB running?")
        exit(1)

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
    parser.add_argument("--show-all-output", action="store_true", help="show all stdout/stderr from the tests as they're running")
    parser.add_argument("--parallel", action="store_true", help="run test classes in parallel")
    parser.add_argument("-j", dest='jobs', help="Set number of jobs for parallel test runs. Default is `nproc`", type=int, default=0)
    parser.add_argument('-k', dest='testNamePatterns',
                        action='append', type=_convert_select_pattern,
                        help='Only run tests which match the given substring')
    parser.add_argument("-x", dest="exclude", nargs="*", default=[])
    args = parser.parse_args()

    print("Compiling spacetime cli...")
    check_call(["cargo", "build"], cwd=TEST_DIR.parent)

    os.environ["SPACETIME_SKIP_CLIPPY"] = "1"

    build_template_target()

    if args.docker:
        docker_container = check_docker()
        # have docker logs print concurrently with the test output
        subprocess.Popen(["docker", "logs", "-f", docker_container])
        smoketests.HAVE_DOCKER = True

    add_prefix = lambda testlist: [TESTPREFIX + test for test in testlist]
    import fnmatch
    excludelist = add_prefix(args.exclude)
    testlist = add_prefix(args.test)

    loader = ExclusionaryTestLoader(excludelist)
    loader.testNamePatterns = args.testNamePatterns

    tests = loader.loadTestsFromNames(testlist)
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
