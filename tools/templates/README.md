# Template Tools

Scripts for maintaining template READMEs and metadata in the SpacetimeDB repo. Output is consumed by [spacetimedb.com](https://github.com/clockworklabs/spacetime-web) for the templates page.

## Scripts

- **generate-readmes** – Converts quickstart MDX docs to Markdown and writes `templates/<slug>/README.md`. Discovers mappings by parsing `--template X` from quickstart files. Templates can override with a `quickstart` field in `.template.json` (must point to a file in the quickstarts dir).
- **update-jsons** – Updates `builtWith` in each `templates/<slug>/.template.json` from package.json, Cargo.toml, and .csproj manifests
- **generate** – Runs both (readmes first, then jsons)

## Usage

From this directory:

```bash
pnpm install
pnpm run generate
```

Or individually:

```bash
pnpm run generate-readmes
pnpm run update-jsons
```

## When to run

Run after changing quickstart docs (`docs/docs/00100-intro/00200-quickstarts/`) or adding/renaming templates. Commit the generated READMEs and updated `.template.json` files.
