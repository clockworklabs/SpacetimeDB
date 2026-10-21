"""Exercise the updater with fake rustup binaries, without network access."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("ensure-rustup.sh")
MOCK_RUSTUP = """#!/usr/bin/env bash
set -eu
if [[ "$*" == --version ]]; then
  echo "rustup $(cat "$MOCK_DIR/version") (test)"
elif [[ "$*" == "self update" ]]; then
  echo called > "$MOCK_DIR/updated"
  echo "$MOCK_UPDATED" > "$MOCK_DIR/version"
else
  exit 99
fi
"""


class EnsureRustupTests(unittest.TestCase):
    def test_update_decisions_and_post_update_verification(self):
        cases = [
            ("1.28.2", "1.29.2", True, True),
            ("1.29.1", "1.29.2", True, True),
            ("1.29.2", "1.29.2", False, True),
            ("1.29.3", "1.29.3", False, True),
            ("1.30.0", "1.30.0", False, True),
            ("2.0.0", "2.0.0", False, True),
            ("1.9.9", "1.29.2", True, True),
            ("1.29.2-beta.1", "1.29.2", True, True),
            ("1.29.1", "1.29.1", True, False),
        ]
        for initial, updated, expect_update, success in cases:
            with self.subTest(initial=initial, updated=updated):
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    (root / "version").write_text(initial)
                    rustup = root / "rustup"
                    rustup.write_text(MOCK_RUSTUP)
                    rustup.chmod(0o755)
                    env = dict(
                        os.environ,
                        PATH=directory + os.pathsep + os.environ["PATH"],
                        MOCK_DIR=directory,
                        MOCK_UPDATED=updated,
                    )
                    result = subprocess.run(
                        ["bash", str(SCRIPT)], env=env, capture_output=True, text=True
                    )
                    self.assertEqual(
                        result.returncode == 0, success, result.stdout + result.stderr
                    )
                    self.assertEqual((root / "updated").exists(), expect_update)
                    self.assertIn("Rustup before:", result.stdout)
                    self.assertIn("Rustup after:", result.stdout)

    def test_missing_rustup_is_installed_without_a_toolchain(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary_dir = root / "bin"
            binary_dir.mkdir()
            cargo_home = root / "cargo"
            (cargo_home / "bin").mkdir(parents=True)
            # Exclude the real rustup while retaining the tools used by the script.
            for tool in ["bash", "sh", "cat"]:
                (binary_dir / tool).symlink_to(shutil.which(tool))
            curl = binary_dir / "curl"
            curl.write_text("""#!/bin/sh
cat <<'INSTALLER'
#!/bin/sh
[ "$*" = "-y --no-modify-path --default-toolchain none" ] || exit 91
printf '#!/bin/sh\\necho "rustup 1.29.2 (test)"\\n' > "$CARGO_HOME/bin/rustup"
/bin/chmod +x "$CARGO_HOME/bin/rustup"
INSTALLER
""")
            curl.chmod(0o755)
            github_path = root / "github-path"
            env = dict(
                os.environ,
                PATH=str(binary_dir),
                CARGO_HOME=str(cargo_home),
                GITHUB_PATH=str(github_path),
                RUNNER_OS="Linux",
            )
            result = subprocess.run(
                ["/bin/bash", str(SCRIPT)], env=env, capture_output=True, text=True
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Rustup before: not installed", result.stdout)
            self.assertEqual(github_path.read_text().strip(), str(cargo_home / "bin"))


if __name__ == "__main__":
    unittest.main()
