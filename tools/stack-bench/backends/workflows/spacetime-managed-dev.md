---
name: spacetime-managed-dev
description: Use the supplied command to manage the SpacetimeDB development watcher.
---

# Development workflow

Development watcher support is available at `/deps/spacetime-dev`. It has not
started yet. Create the module and `/app/spacetime.json` first. Configure the
supplied server URL and database name, your `module-path`, and TypeScript
`generate` targets with their `out-dir` paths. Keep these paths inside `/app`.
This helper supports one database per application. See the CLI skill for configuration syntax.

Run `/deps/spacetime-dev start`. It starts one watcher with data deletion disabled.
Repeated calls report the existing watcher. Run `/deps/spacetime-dev status` to
check startup, and read the reported log for build or publish errors.
"Starting" does not mean the initial publish has completed. "Running" confirms
the initial publish and bindings; later edits can still fail, so check the log.
Start the frontend separately. Do not start another watcher or competing publisher.

Use `/deps/spacetime-dev stop` before changing configuration or testing a clean
startup, then `start` again when needed. An exited watcher is not restarted
automatically. Keep `/app/start.sh` able to build and start the complete application
without this development session. Container cleanup stops the watcher.
