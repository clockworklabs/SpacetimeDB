import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
import toml

from .. import STDB_DIR, spacetime, pnpm, requires_dotnet, run_cmd


class TestSpacetimeInit(unittest.TestCase):
    def get_templates(self):
        templates_dir = STDB_DIR / "templates"
        templates = []

        for entry in templates_dir.iterdir():
            if not entry.is_dir():
                continue

            template_json = entry / ".template.json"
            if not template_json.exists():
                continue

            with open(template_json) as f:
                metadata = json.load(f)

            templates.append({
                "id": entry.name,
                "server_lang": metadata["server_lang"],
                "client_lang": metadata.get("client_lang"),
            })

        return templates

    def test_init_and_publish_all_templates(self):
        templates = self.get_templates()
        self.assertGreater(len(templates), 0, "No templates found")

        print(f"\nTesting {len(templates)} template(s)...")
        print("="*60)

        results = {}
        passed_count = 0
        failed_count = 0

        for i, template in enumerate(templates, 1):
            print(f"\n[{i}/{len(templates)}] Testing template: {template['id']}")
            print("-" * 60)
            with self.subTest(template=template["id"]):
                try:
                    self._test_template(template)
                    results[template["id"]] = "[PASS]"
                    passed_count += 1
                    print(f"[PASS] {template['id']}")
                except Exception as e:
                    results[template["id"]] = f"[FAIL]: {str(e)}"
                    failed_count += 1
                    print(f"[FAIL] {template['id']}: {str(e)}")
                    raise

        print("\n" + "="*60)
        print("TEMPLATE TEST SUMMARY")
        print("="*60)
        for template_id, result in sorted(results.items()):
            print(f"{template_id:30} {result}")
        print("="*60)

        total = len(results)
        print(f"TOTAL: {passed_count}/{total} passed\n")

    def _test_template(self, template):
        with tempfile.TemporaryDirectory() as tmpdir:
            project_name = f"test-{template['id']}"
            project_path = Path(tmpdir) / project_name

            print(f"  > Initializing project from template...")
            spacetime(
                "init",
                "--template", template["id"],
                "--project-path", str(project_path),
                "--non-interactive",
                project_name
            )

            self.assertTrue(project_path.exists(), f"Project directory not created for {template['id']}")

            if template.get("server_lang"):
                server_path = project_path / "spacetimedb"
                self.assertTrue(server_path.exists(), f"Server directory not found for {template['id']}")

                print(f"  > Publishing server...")
                self._publish_server(template, server_path)

            if template.get("client_lang"):
                print(f"  > Testing client...")
                self._test_client(template, project_path)

    def _publish_server(self, template, server_path):
        server_lang = template["server_lang"]
        template_id = template["id"]

        if server_lang == "typescript":
            self._setup_typescript_local_sdk(server_path)
            pnpm("install", cwd=server_path)

        if server_lang == "rust":
            self._setup_rust_local_sdk(server_path)

        if server_lang == "csharp":
            self._setup_csharp_nuget(server_path)

        domain = f"test-{server_lang}-{os.urandom(8).hex()}"
        print(f"  > Building and publishing template '{template_id}' (language: {server_lang}) at {server_path}")
        spacetime("publish", "-s", "local", "--yes", "--project-path", str(server_path), domain)

        spacetime("delete", "-s", "local", "--yes", domain)

    def _update_cargo_toml_dependency(self, cargo_toml_path, package_name, local_path):
        """Replace crates.io dependency with local path dependency."""
        if not cargo_toml_path.exists():
            return

        cargo_data = toml.load(cargo_toml_path)

        if package_name in cargo_data.get("dependencies", {}):
            cargo_data["dependencies"][package_name] = {"path": str(local_path)}

            with open(cargo_toml_path, 'w') as f:
                toml.dump(cargo_data, f)

    def _setup_rust_local_sdk(self, server_path):
        """Replace crates.io spacetimedb dependency with local path dependency."""
        print(f"  > Setting up local Rust SDK...")
        cargo_toml_path = server_path / "Cargo.toml"
        rust_sdk_path = STDB_DIR / "crates/bindings"
        self._update_cargo_toml_dependency(cargo_toml_path, "spacetimedb", rust_sdk_path)

    def _update_package_json_dependency(self, package_json_path, package_name, local_path):
        """Replace npm package dependency with local path reference."""
        with open(package_json_path, 'r') as f:
            package_data = json.load(f)

        # Convert to absolute path and format as URI for npm/pnpm file: protocol
        abs_path = Path(local_path).absolute()
        # Use as_uri() to get proper file:// URL format (works on both Windows and Unix)
        file_url = abs_path.as_uri()
        package_data["dependencies"][package_name] = file_url

        with open(package_json_path, 'w') as f:
            json.dump(package_data, f, indent=2)

    def _setup_typescript_local_sdk(self, server_path):
        """Replace npm registry spacetimedb dependency with local SDK path reference."""
        print(f"  > Setting up local TypeScript SDK...")
        typescript_sdk_path = STDB_DIR / "crates/bindings-typescript"
        print(f"  > Building TypeScript SDK...")
        pnpm("install", cwd=typescript_sdk_path)
        pnpm("build", cwd=typescript_sdk_path)

        # Create a global link from the SDK
        print(f"  > Linking TypeScript SDK globally...")
        pnpm("link", "--global", cwd=typescript_sdk_path)

        # Link it in the server project
        print(f"  > Linking spacetimedb package in server...")
        pnpm("link", "--global", "spacetimedb", cwd=server_path)

        # Remove lockfile since the linked version may differ from lockfile spec
        lockfile = server_path / "pnpm-lock.yaml"
        if lockfile.exists():
            lockfile.unlink()

    def _setup_csharp_nuget(self, server_path):
        """Create a local nuget.config file to avoid polluting global NuGet sources"""
        print(f"  > Setting up C# NuGet sources...")
        nuget_config = server_path / "nuget.config"
        if not nuget_config.exists():
            nuget_config.write_text("""<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
""")

        bindings = STDB_DIR / "crates/bindings-csharp"
        packed_projects = ["BSATN.Runtime", "Runtime", "BSATN.Codegen", "Codegen"]

        for project in packed_projects:
            run_cmd("dotnet", "pack", "-c", "Release", cwd=bindings / project)
            path = bindings / project / "bin" / "Release"
            project_name = f"SpacetimeDB.{project}"
            run_cmd("dotnet", "nuget", "add", "source", str(path), "-n", project_name, "--configfile", str(nuget_config), cwd=server_path)

        # Pack ClientSDK for client projects
        client_sdk = STDB_DIR / "sdks/csharp"
        client_sdk_proj = client_sdk / "SpacetimeDB.ClientSDK.csproj"
        run_cmd("dotnet", "pack", str(client_sdk_proj), "-c", "Release")
        client_sdk_path = client_sdk / "bin~" / "Release"
        run_cmd("dotnet", "nuget", "add", "source", str(client_sdk_path), "-n", "SpacetimeDB.ClientSDK", "--configfile", str(nuget_config), cwd=server_path)

    def _test_client(self, template, project_path):
        """Test the client code based on the client language."""
        client_lang = template.get("client_lang")

        if client_lang == "rust":
            print(f"    - Building Rust client...")
            # Setup local SDK for client
            client_cargo_toml = project_path / "Cargo.toml"
            rust_sdk_path = STDB_DIR / "sdks/rust"
            self._update_cargo_toml_dependency(client_cargo_toml, "spacetimedb-sdk", rust_sdk_path)
            run_cmd("cargo", "build", cwd=project_path)

        elif client_lang == "typescript":
            print(f"    - Type-checking TypeScript client...")
            # Link the globally linked spacetimedb package
            pnpm("link", "--global", "spacetimedb", cwd=project_path)
            # Remove lockfile since the linked version may differ from lockfile spec
            lockfile = project_path / "pnpm-lock.yaml"
            if lockfile.exists():
                lockfile.unlink()
            # Install other dependencies
            pnpm("install", cwd=project_path)
            # Run TypeScript compiler in check mode
            pnpm("exec", "tsc", "--noEmit", cwd=project_path)

        elif client_lang == "csharp":
            print(f"    - Building C# client...")
            # Setup nuget for client if needed
            self._setup_csharp_nuget(project_path)
            run_cmd("dotnet", "build", cwd=project_path)
