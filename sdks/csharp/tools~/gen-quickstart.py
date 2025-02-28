import os
import sys
import subprocess

def main():
    if len(sys.argv) < 2:
        print("Usage: python script.py <STDB_PATH>")
        sys.exit(1)

    stdb_path = sys.argv[1]
    sdk_path = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    
    # Define the command
    command = [
        "cargo", "run", "--manifest-path", os.path.join(stdb_path, "crates/cli/Cargo.toml"),
        "--", "generate", "-y", "-l", "csharp", "-o",
        os.path.join(sdk_path, "examples~/quickstart-chat/client/module_bindings"),
        "--project-path", os.path.join(stdb_path, "modules/quickstart-chat")
    ]
    
    # Run the command
    subprocess.run(command, check=True)

if __name__ == "__main__":
    main()
