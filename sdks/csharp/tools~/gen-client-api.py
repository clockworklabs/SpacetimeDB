import os
import sys
import subprocess
import shutil


def main():
    if len(sys.argv) < 2:
        print("Usage: python script.py <STDB_PATH>")
        sys.exit(1)
    
    print("Usage: python script.py <STDB_PATH>")

    stdb_path = sys.argv[1]
    sdk_path = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

    # Run the first cargo command
    print("Getting the WebSocket Schema from client-api-messages crate.")
    get_ws_schema = subprocess.Popen(
        [
            "cargo", "run", "--manifest-path",
            os.path.join(stdb_path, "crates", "client-api-messages", "Cargo.toml"),
            "--example", "get_ws_schema"
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )

    # Run the second cargo command using the output of the first
    print("Passing the WebSocket Schema to the CLI to generate the SpacetimeDB.ClientApi.")
    generate_process = subprocess.Popen(
        [
            "cargo", "run", "--manifest-path",
            os.path.join(stdb_path, "crates", "cli", "Cargo.toml"),
            "--", "generate", "-l", "csharp", "--namespace", "SpacetimeDB.ClientApi",
            "--module-def", "-o", os.path.join(sdk_path, "src", "SpacetimeDB", "ClientApi", ".output")
        ],
        stdin=get_ws_schema.stdout,
        stderr=subprocess.PIPE
    )

    get_ws_schema.stdout.close()
    generate_process.communicate()

    # Move generated files
    print("Moving the generated Types files to the ClientApi directory.")
    output_dir = os.path.join(sdk_path, "src", "SpacetimeDB", "ClientApi", ".output", "Types")
    target_dir = os.path.join(sdk_path, "src", "SpacetimeDB", "ClientApi")
    if os.path.exists(output_dir):
        for filename in os.listdir(output_dir):
            shutil.move(os.path.join(output_dir, filename), os.path.join(target_dir, filename))
        print("Removing .output directory.")
        shutil.rmtree(os.path.join(sdk_path, "src", "SpacetimeDB", "ClientApi", ".output"))

if __name__ == "__main__":
    main()
