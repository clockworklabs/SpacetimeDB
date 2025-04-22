import tomli
import argparse
import sys
from pathlib import Path

def find_non_path_spacetimedb_deps(cargo_toml_path):
    cargo_toml = Path(cargo_toml_path)
    if not cargo_toml.exists():
        raise FileNotFoundError(f"{cargo_toml_path} not found.")

    with cargo_toml.open("rb") as f:
        data = tomli.load(f)

    dev_deps = data.get("dev-dependencies", {})
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

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Check dev-dependencies in Cargo.toml for non-path spacetimedb deps.")
    parser.add_argument("directory", help="Directory to search for Cargo.toml")

    args = parser.parse_args()
    cargo_toml_path = Path(args.directory) / "Cargo.toml"

    try:
        deps = find_non_path_spacetimedb_deps(cargo_toml_path)
        if deps:
            print(f"❌ Non-path `spacetimedb` dev-dependencies found in {cargo_toml_path}:")
            for dep in deps:
                print(f"  - {dep}")
            sys.exit(1)
        else:
            print(f"✅ All `spacetimedb` dev-dependencies in {cargo_toml_path} are path-based.")
            sys.exit(0)
    except Exception as e:
        print(f"⚠️ Error: {e}")
        sys.exit(2)
