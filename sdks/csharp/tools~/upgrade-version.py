import os
import re
import sys
import json
import argparse

def update_csproj_file(path, version):
    with open(path, 'r', encoding='utf-8') as f:
        lines = f.readlines()

    updated_lines = []
    version_prefix = '.'.join(version.split('.')[:2]) + '.*'
    version_tag_written = set()

    for i, line in enumerate(lines):
        # Update PackageReference for SpacetimeDB.*
        updated_line = re.sub(
            r'(<PackageReference[^>]*Include="SpacetimeDB\.[^"]*"[^>]*Version=")[^"]*(")',
            rf'\g<1>{version_prefix}\g<2>',
            line
        )

        for tag in ('Version', 'AssemblyVersion'):
            if re.search(rf'<{tag}>.*</{tag}>', updated_line.strip()):
                updated_line = re.sub(
                    rf'(<{tag}>).*(</{tag}>)',
                    rf'\g<1>{version}\g<2>',
                    updated_line
                )
                version_tag_written.add(tag)

        updated_lines.append(updated_line)

    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(updated_lines)
    print(f"Updated: {path}")

def update_all_csproj_files(version):
    for root, _, files in os.walk('.'):
        for file in files:
            if file.endswith('.csproj'):
                update_csproj_file(os.path.join(root, file), version)

def update_package_json(version):
    path = 'package.json'
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        data['version'] = version
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2)
            f.write('\n')  # ensure trailing newline
        print(f"Updated version in {path} to {version}")
    else:
        print("package.json not found.")

def main():
    parser = argparse.ArgumentParser(
        description='Update version numbers in .csproj files and package.json',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python upgrade-version.py 1.2.3
  python upgrade-version.py --version 1.2.3
        """
    )
    parser.add_argument(
        'version',
        help='Version number to set (e.g., 1.2.3)'
    )

    args = parser.parse_args()

    update_all_csproj_files(args.version)
    update_package_json(args.version)

if __name__ == '__main__':
    main()
