import os
import sys

def main():
    if len(sys.argv) < 2:
        print("Usage: python script.py <SPACETIMEDB_REPO_PATH>")
        sys.exit(1)
    
    spacetime_repo_path = sys.argv[1]
    script_dir = os.path.dirname(os.path.abspath(__file__))
    sdk_path = os.path.abspath(os.path.join(script_dir, ".."))
    
    nuget_config_path = os.path.join(sdk_path, "nuget.config")
    
    nuget_config_content = f'''<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <!-- Local NuGet repositories -->
    <add key="Local SpacetimeDB.BSATN.Runtime" value="{spacetime_repo_path}/crates/bindings-csharp/BSATN.Runtime/bin/Release" />
  </packageSources>
  <packageSourceMapping>
    <!-- Ensure that SpacetimeDB.BSATN.Runtime is used from the local folder. -->
    <!-- Otherwise we risk an outdated version being quietly pulled from NuGet for testing. -->
    <packageSource key="Local SpacetimeDB.BSATN.Runtime">
      <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <!-- Fallback for other packages (e.g. test deps). -->
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
'''
    
    with open(nuget_config_path, "w", encoding="utf-8") as f:
        f.write(nuget_config_content)
    
    print("Wrote nuget.config contents:")
    print(nuget_config_content)

if __name__ == "__main__":
    main()