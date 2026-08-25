# Vault files example

Vault is a small Drive-style file manager built with
[`@spacetimedb/files`](../). File bytes and file records live in the mounted
Files component; the host module adds identity-owned folder metadata and scoped
views.

## What this demonstrates

- Uploading, moving, renaming, listing, downloading, and deleting files.
- Identity-owned folders and caller-scoped file-summary subscriptions.
- Keeping file bytes out of realtime subscriptions.
- Reading private bytes through a sender-aware procedure.
- Serving explicitly public files through the component HTTP handler.
- Drag-and-drop uploads, folder traversal, search, previews, bulk actions, and ZIP
  downloads in a browser client.

## Prerequisites

- Node.js 20 or later and pnpm 10.
- The released SpacetimeDB 2.8 CLI.
- A local SpacetimeDB server registered as `local`.
- A logged-in CLI identity for publishing the example.

Select the supported CLI release, then keep the local server running in a
separate terminal:

```powershell
spacetime version install 2.8.3
spacetime version use 2.8.3
spacetime start
```

```powershell
spacetime server ping local
spacetime login show
```

## Quick start

From `spacetime-files-ts/example`:

```powershell
pnpm install
pnpm --dir spacetimedb install
node -e "require('node:fs').copyFileSync('.env.example', '.env')"
pnpm run build:module:fresh
pnpm run dev
```

Open <http://127.0.0.1:8799> and upload a small image or text file.

`build:module:fresh` deletes and recreates only the local `spacetime-files-example`
database. Use `pnpm run build:module` when existing local files must be preserved.

## Use in your project

This workspace tests the component source in this repository. Consumer applications install published releases:

```bash
npm install @spacetimedb/files @spacetimedb/crypto spacetimedb@^2.8.3
```

Follow the package's
[integration guide](../README.md#integrate-into-an-application). Copy the
owner-derivation, scoped-view, and download-handler patterns; the folder model
and file-manager UI are application code in the example.

## Configuration

| Variable            | Default                   | Purpose                                          |
| ------------------- | ------------------------- | ------------------------------------------------ |
| `HOST`              | `127.0.0.1`               | Development web-server bind address.             |
| `PORT`              | `8799`                    | Development web-server port.                     |
| `STDB_URI`          | `ws://127.0.0.1:3000`     | Browser WebSocket endpoint.                      |
| `STDB_HTTP`         | `http://127.0.0.1:3000`   | Upstream endpoint for public file HTTP requests. |
| `STDB_APP_DATABASE` | `spacetime-files-example` | Published database name.                         |

The Node server hosts the bundle and proxies `/files?id=<fileId>` to the module HTTP router.
It receives metadata for authorization decisions. Private bytes travel through
the authenticated SpacetimeDB connection.

## Read and write paths

The browser subscribes to `my_folders` and `my_file_summaries`. These views are
filtered by the connection identity, and summaries omit `bytes`. Folder and file
mutations use reducers.

Private content is returned by the `read_file_bytes` procedure. Procedures retain
the real caller in `ctx.sender`, allowing the host module to enforce ownership
before returning bytes over the authenticated SpacetimeDB connection. HTTP
handlers execute with the module route context and serve public files.

Files marked public can use `/files?id=<fileId>` for direct HTTP reads. Making a file public
changes its confidentiality and creates a public download path.

## Paths and limits

- Paths are absolute and slash-prefixed, for example `/docs/readme.txt`.
- File and folder paths are owner-scoped. Two identities can each use `/docs`
  and `/docs/readme.txt`.
- Public links use the stable numeric file ID.
- The component stores bytes in SpacetimeDB rows and caps each file at 4 MB.
- Vault demonstrates in-row storage for small assets. Use dedicated infrastructure
  for streaming uploads, media transformation, backups, and CDN delivery.

The browser stores its development SpacetimeDB identity token so files remain
associated with the same identity after reload. If a fresh database rejects the
token, the client obtains a new anonymous identity. Existing data remains with
its original identity.

## Security and deployment boundaries

- Reducers, views, and private-byte procedures enforce ownership. Browser controls
  provide presentation only.
- Validate path normalization, MIME metadata, file size, and ownership before
  accepting writes or moves.
- Treat uploaded bytes as untrusted. Production systems need content-disposition
  policy, safe MIME handling, malware scanning where appropriate, and defenses
  against active HTML/SVG content.
- Public file URLs are bearer-readable by design. Do not expose confidential files
  by marking them public.
- The example buffers whole files and generated ZIPs in memory. Production limits
  should account for per-file size, concurrent requests, and aggregate memory.
- The included proxy is a local development server. Production needs TLS, explicit
  binding, request limits, origin policy, and process supervision.

## Build and verification

```powershell
pnpm --dir spacetimedb run build
pnpm run check
pnpm run build
```

For a release smoke test, use two independent browser identities and verify:

1. Upload, preview, download, rename, move, and delete each supported small file
   type.
2. Folder drag-and-drop and multi-selection perform the intended operation once.
3. Private file bytes and summaries are invisible to the other identity.
4. A public file is reachable through `/files?id=<fileId>`; an owner-only file returns 403.
5. Oversized uploads and invalid or conflicting paths fail atomically.
6. Refresh preserves the owning development identity unless the database was
   deliberately reset.

## Troubleshooting

- **A preview is empty:** inspect the procedure failure and verify the connected
  identity owns the file.
- **A public link returns an error:** confirm `STDB_HTTP` and `STDB_APP_DATABASE`
  target the database used by `STDB_URI`.
- **An upload exceeds the limit:** keep example files below 4 MB; use an external
  object store for larger production assets.
- **Files disappear after a fresh publish:** `build:module:fresh` deliberately
  replaces the local database and all of its rows.

## Important files

- `spacetimedb/src/index.ts` - Files mount, folders, scoped views, and private reads.
- `src/app.ts` - file-manager state, uploads, previews, downloads, and subscriptions.
- `server.ts` - static development server and public-file proxy.
- `public/index.html` - Vault interface.
- `public/styles.css` - Vault presentation.
