# Building a new TypeScript submodule

These are the rules for the `spacetime-<name>-ts` packages in this folder, except `spacetime-example-ui-ts`, which holds the shared example UI. The other modules here are test and benchmark fixtures.

Copy what exists. Do not invent structure, UI or copy that the other modules don't have.

Read the current code on `origin/master` first. The closest models:

- `spacetime-retry-ts`: small factory with no example.
- `spacetime-workflow-ts`: factory with a scheduled procedure and a live restart test.
- `spacetime-cron-ts`: factory with an example and a live restart test.
- `spacetime-rate-limit-ts`, `spacetime-agents-ts`: mountable submodules.

## Choose the shape

- **Factory** (`client({...})`, spread `...x.tables` into the host schema): when the module needs host types, or runs host code in the host's transaction.
- **Mountable** (`import * as x from '@spacetimedb/x/submodule'`, `schema({ x })`): when it owns its tables and functions outright.
- **Helpers:** pure functions, no tables.

## Package

Copy from `spacetime-retry-ts`, then change the names:

- `.npmrc`, `tsconfig.json`, `tsconfig.build.json`, `package.json`, `spacetimedb/{.npmrc,package.json,tsconfig.json}`.
- `LICENSE.txt` must be a real symlink to `../../licenses/apache2.txt` (git mode 120000). `cp -P` on Windows copies the file, so use `MSYS=winsymlinks:nativestrict ln -s`.
- `package.json`: `submodule.category` must be one the site accepts (`app/data/submodules.ts` in `spacetime-web`). Keep `publishConfig`, `prepack`, `postpack`.
- `pnpm-workspace.yaml` already covers `modules/spacetime-*-ts`.

## Code conventions

- Export an `errors` object of `<name>.*` codes, thrown as `SenderError`s.
- `install(ctx)` seeds the publisher as admin. Admin tables, views and reducers follow retry's pattern.
- Tables are private. Hosts expose data through views.
- Everything that grows has a cap. Retry and workflow keep 1,000 rows and prune before inserting.
- Scheduled functions check `ctx.sender.isEqual(ctx.databaseIdentity)`. Clients can otherwise call them directly.
- Outside calls go in procedures, and are at-least-once.

## Platform behavior to design around

- Handlers are synchronous.
- The host deletes a scheduled **procedure's** row and commits **before** calling it. A crash then loses the row unless something else is scheduled.
- A scheduled **reducer** that catches an exception commits its partial writes. Only rethrowing rolls back, and then you can't record the failure in that transaction.
- `ctx.withTx` rolls back if its body throws, then the procedure can catch it and record the failure in a new `withTx`. Its body may run twice if the commit fails.
- Index scans yield rows inserted in the current transaction before committed rows.

## Tests

- `scripts/test.ts` with `scripts/sys-abi-test-register.mjs`, using a mock context. CI runs `lint`, `typecheck`, `test` and `build` for all modules by glob.
- If restarts matter, add `scripts/test-recovery.mjs` on its own server and port, and one line in `tools/ci/commands/typescript-submodules/src/main.rs`. Never use the user's server on port 3000.
- Use `@ts-expect-error` for type guarantees, without `as never`, which hides them.
- Do not end the unit test with `console.log`. `spacetimedb/server` replaces it and it crashes under the stub.

## Example app

Most modules ship an example in `example/`. Before writing one:

1. Look at every other example, then choose the one closest to what this app does. Use Agents for AI or chat, Dispatch for a form with a live feed, and Vault for file lists. If it isn't obvious, ask.
2. Read all of that example's `index.html`, `styles.css` and app code.
3. Start from its stylesheet and remove the rules you don't use. Do not write page CSS from scratch, and do not mix layouts.

Then:

- Make it a small, concrete product with a short name, not a feature demo. It needs a real use for the module, not fake steps.
- Import `@spacetimedb/example-ui/styles.css` in `src/app.ts` and link `./app.css` before `./styles.css`. Use its `--color-*`, `--font-*` and `--radius-*` tokens. Never add your own palette. Primary buttons are grey (`--color-n3`) and only turn green when pressed.
- Add nothing the chosen example doesn't have, such as connection dots or extra copy. Show connection problems only while they happen.
- Use the shared auth panel (`mountAuthPanel`) if the example has sign-in. Ask whether it needs sign-in if it spends paid credits.
- Copy `package.json`, `server.ts`, `.env.example`, `tsconfig.json` and the `spacetimedb/` setup from an existing example, and pick an unused port.
- Procedures cannot call local or private addresses, so a fake endpoint in `server.ts` will not work.
- Run it in a browser before calling it done, at desktop and phone width. Compile checks do not catch missing updates: listen for `onInsert`, `onUpdate` and `onDelete`.
- Test on an isolated server with its own port and data directory, never the user's on port 3000.

## Docs

- README sections in order: Install, Integrate into an application, the module's own sections, API, Testing, License. Compile the example with placeholders stubbed.
- Add a row to `docs/docs/00250-how-to/00800-migrating-from-convex.md` if Convex has an equivalent.
- Add it to the library list in both `skills/typescript-server/SKILL.md` and `codex-plugin/plugins/spacetimedb/skills/typescript-server/SKILL.md`.

## Lockfile

`pnpm install` on Windows also changes unrelated lines and drops `libc:` entries. Keep the diff to the new importer blocks: take the base lockfile, insert only those blocks, and confirm with `pnpm install --frozen-lockfile --ignore-scripts`. If you hit `ERR_PNPM_MISSING_TIME`, pass `--config.cache-dir=<fresh dir>`.

## Before calling it done

- `pnpm run lint`, `typecheck`, `test`, `build`, and `test:recovery` if it has one.
- `pnpm pack`: the tarball holds only `dist/`, `LICENSE`, `README.md`, `package.json`, and `LICENSE` is Apache.
- Publishing and listing on the site: use the `publish-submodules` skill.
