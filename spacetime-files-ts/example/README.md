# Vault files example

A small file manager. Upload files, organize them in folders, preview them,
and download them. Files can be private or shared with a public link.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-files-ts/example`, copy [.env.example](./.env.example) to `.env`.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8799>.

## Try it

1. Upload a small image or text file. Each file must be under 4 MB.
2. Preview it, rename it, and download it.
3. Create a folder and move the file into it.
4. Open the app in a private browser window. That browser has a separate file list.
5. Back in the first window, select a file and choose **Make public**. Use **Copy link** to
   get its public download URL.

Anyone with a public download URL can read that file. Use only sample files
when trying public sharing.

The app remembers your browser identity across reloads. Clearing site data
gives you a new identity; it does not transfer your old files to that identity.

## Configuration

The defaults in [.env.example](./.env.example) work for a local server.
`STDB_URI` and `STDB_HTTP` must point to the same SpacetimeDB instance.

## Before deploying

Add accounts and account recovery if users need lasting access to their files.
Treat uploaded content as untrusted, especially HTML and SVG.
This demo holds files and ZIP downloads in memory and stores file bytes in
SpacetimeDB. Plan storage and download limits for your app's expected file sizes.

## Troubleshooting

- **Upload is rejected:** try a file under 4 MB and check for a duplicate path.
- **Public link fails:** check that the file is public and both database endpoints
  point to the same server.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): folders and file access.
- [src/app.ts](./src/app.ts): uploads, previews, and downloads.
- [server.ts](./server.ts): public download routing.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-files-example` database.**

To use the submodule in your own app, see the
[package integration guide](../README.md#integrate-into-an-application).
