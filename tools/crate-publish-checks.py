import toml
import argparse
import sys
from pathlib import Path

def find_non_path_spacetimedb_deps(dev_deps):
    non_path_spacetimedb = []
    for name, details in dev_deps.items():
        if not name.startswith("spacetimedb"):
            continue

        if isinstance(details, dict):
            if "path" not in details:
                non_path_spacetimedb.append(name)
        else:
            # String dependency = version from crates.io
            non_path_spacetimedb.append(name)
    return non_path_spacetimedb

def check_cargo_metadata(data):
    package = data.get("package", {})
    missing_fields = []

    # Accept either license OR license-file
    if "license" not in package and "license-file" not in package:
        missing_fields.append("license/license-file")

    if "description" not in package:
        missing_fields.append("description")

    return missing_fields

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Check Cargo.toml for metadata and dev-dependencies.")
    parser.add_argument("directory", help="Directory to search for Cargo.toml")

    args = parser.parse_args()
    cargo_toml_path = Path(args.directory) / "Cargo.toml"

    try:
        if not cargo_toml_path.exists():
            raise FileNotFoundError(f"{cargo_toml_path} not found.")

        data = toml.load(cargo_toml_path)

        # Check dev-dependencies
        dev_deps = data.get("dev-dependencies", {})
        bad_deps = find_non_path_spacetimedb_deps(dev_deps)

        # Check license/license-file and description
        missing_fields = check_cargo_metadata(data)

        exit_code = 0

        if bad_deps:
            print(f"❌ These dev-dependencies in {cargo_toml_path} must be converted to use `path` in order to not impede crate publishing:")
            for dep in bad_deps:
                print(f"  - {dep}")
            exit_code = 1

        if missing_fields:
            print(f"❌ Missing required fields in [package] of {cargo_toml_path}: {', '.join(missing_fields)}")
            exit_code = 1

        if exit_code == 0:
            print(f"✅ {cargo_toml_path} passed all checks.")

        sys.exit(exit_code)

    except Exception as e:
        print(f"⚠️ Error: {e}")
        sys.exit(2)
