import toml
import argparse
import sys
from pathlib import Path

def check_deps(dev_deps, cargo_toml_path):
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
    success = not non_path_spacetimedb
    return success, non_path_spacetimedb

def check_package_metadata(package, cargo_toml_path):
    missing_fields = []

    # Accept either license OR license-file
    if "license" not in package and "license-file" not in package:
        missing_fields.append("license/license-file")

    if "description" not in package:
        missing_fields.append("description")

    missing_license_file = None
    if "license-file" in package:
        license_file = package["license-file"]
        license_path = cargo_toml_path.parent / license_file
        if not license_path.exists():
            missing_license_file = license_path

    success = not missing_fields and not missing_license_file
    return success, missing_fields, missing_license_file

def run_checks(data, cargo_toml_path):
    result = {
        "success": True,
    }

    success, bad_deps = check_deps(data.get("dev-dependencies", {}), cargo_toml_path)
    result["success"] = result["success"] and success
    result["bad_deps"] = bad_deps

    success, missing_fields, missing_license_file = check_package_metadata(data.get("package", {}), cargo_toml_path)
    result["missing_fields"] = missing_fields
    result["missing_license_file"] = missing_license_file
    result["success"] = result["success"] and success

    return result

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Check Cargo.toml for metadata and dev-dependencies.")
    parser.add_argument("directory", help="Directory to search for Cargo.toml")

    args = parser.parse_args()
    cargo_toml_path = Path(args.directory) / "Cargo.toml"

    try:
        if not cargo_toml_path.exists():
            raise FileNotFoundError(f"{cargo_toml_path} not found.")

        data = toml.load(cargo_toml_path)

        checks = run_checks(data, cargo_toml_path)
        if checks["success"]:
            print(f"✅ {cargo_toml_path} passed all checks.")
        else:
            print(f"❌ {cargo_toml_path} failed checks:")
            if checks["missing_fields"]:
                print(f"  Missing required fields: {', '.join(checks['missing_fields'])}")
            if checks["missing_license_file"]:
                print(f"  Specified license file does not exist: {checks['missing_license_file']}")
            if checks["bad_deps"]:
                print(f"  These dev-dependencies must be converted to use `path` in order to not impede crate publishing: {', '.join(checks['bad_deps'])}")
            sys.exit(1)

    except Exception as e:
        print(f"⚠️ Error: {e}")
        sys.exit(2)
