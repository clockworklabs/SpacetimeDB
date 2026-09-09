---
name: spacetime-dev
description: Use the SpacetimeDB development watcher while implementing an application.
---

# Development workflow

Use `spacetime dev` while implementing and repairing the application. Keep one
watcher running for the assigned database. It builds module changes, publishes
them, and generates client bindings. Use the supplied CLI, server URI, database
name, and module directory. See the CLI skill for command syntax.

Use `--yes --delete-data=never`, TypeScript client bindings, and the application's
actual bindings directory. Use `--server-only` if you start the web client
separately. If `spacetime.json` already defines publish targets, use those paths
and omit `--module-path`. Do not run competing publish commands or watchers.

Wait for the initial publish and bindings to succeed before opening the app.
After a module edit, check that the watcher published it successfully before
checking app behavior. Fix watcher errors; do not assume that the live module is current.
Restart the watcher if it exited or its configuration changed.

Keep `/app/start.sh` able to build and start the complete application from a
clean source checkout without this development session. Stop the watcher before
checking that startup path. One-shot commands remain appropriate for that script
and for diagnosing a watcher failure.
