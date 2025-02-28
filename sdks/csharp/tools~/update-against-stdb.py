import os
import sys
import subprocess
import shutil

def run_command(command, shell=False):
    output = subprocess.run(command, shell=shell, check=True, capture_output=True, text=True)
    print("Command: " + " ".join(output.args))
    print("Output:")
    print(output.stdout)

def main():
    if len(sys.argv) < 2:
        print("Usage: python script.py <STDB_PATH>")
        sys.exit(1)
    
    stdb_path = sys.argv[1]
    sdk_path = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    cwd_path = os.getcwd()
    
    # Run necessary scripts
    print("Running: write-nuget-config.py")
    run_command(["python", os.path.join(sdk_path, "tools~", "write-nuget-config.py"), stdb_path])
    print("Running: gen-client-api.py")
    run_command(["python", os.path.join(sdk_path, "tools~", "gen-client-api.py"), stdb_path])
    print("Running: gen-quickstart.py")
    run_command(["python", os.path.join(sdk_path, "tools~", "gen-quickstart.py"), stdb_path])
    
    # Clear nuget cache
    print("Clearing existing NuGet cache.")
    run_command(["dotnet", "nuget", "locals", "all", "--clear"])
    
    # Pack bindings
    print("Packing binding DLLs into SpacetimeDB/crates/bindings-csharp")
    run_command(["dotnet", "pack", os.path.join(stdb_path, "crates/bindings-csharp")])
    
    # Remove and repack packages
    packages_path = os.path.join(sdk_path, "packages")
    if os.path.exists(packages_path):
        print("Removing old NuGet packages.")
        shutil.rmtree(packages_path)
    print("Packing code into NuGet packages.")
    run_command(["dotnet", "pack", sdk_path])
    
    # Run tests
    print("Executing Unit Tests.")
    run_command(["dotnet", "test", sdk_path])
    
    # Reset specific git-tracked files
    print("Resetting git-tracked files.")
    os.chdir(sdk_path)
    run_command(["git", "checkout", "--", "packages/*.meta", "packages/**/*.meta", "packages/.gitignore"])
    os.chdir(cwd_path)

if __name__ == "__main__":
    main()
