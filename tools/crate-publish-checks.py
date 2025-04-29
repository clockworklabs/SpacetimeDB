import toml
import argparse
import sys
from pathlib import Path

def check_deps(dev_deps):
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
    if non_path_spacetimedb:
        print(f"❌ These dev-dependencies must be converted to use `path` in order to not impede crate publishing:")
        for dep in non_path_spacetimedb:
            print(f"  - {dep}")
        return False
    return True

def check_package_metadata(package, cargo_toml_path):
    has_errors = False

    # Accept either license OR license-file
    if "license" not in package and "license-file" not in package:
        print(f"❌ Missing required field: license/license-file")
        has_errors = True

    if "license-file" in package:
        license_file = package["license-file"]
        license_path = cargo_toml_path.parent / license_file
        if not license_path.exists():
            print(f"❌ License file '{license_file}' specified in Cargo.toml does not exist")
            has_errors = True

    if "description" not in package:
        print(f"❌ Missing required field: description")
        has_errors = True

    return not has_errors

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Check Cargo.toml for metadata and dev-dependencies.")
    parser.add_argument("directory", help="Directory to search for Cargo.toml")

    args = parser.parse_args()
    cargo_toml_path = Path(args.directory) / "Cargo.toml"

    try:
        if not cargo_toml_path.exists():
            raise FileNotFoundError(f"{cargo_toml_path} not found.")

        data = toml.load(cargo_toml_path)

        dev_deps = data.get('dev-dependencies', {})
        package = data.get('package', {})
        if check_deps(dev_deps) and check_package_metadata(package, cargo_toml_path):
            print(f"✅ {cargo_toml_path} passed all checks.")
        else:
            sys.exit(1)

    except Exception as e:
        print(f"⚠️ Error: {e}")
        sys.exit(2)
