---
name: spacetime-dev
description: Use the SpacetimeDB development watcher while implementing an application.
---

# Development workflow

Use `spacetime dev` while implementing and repairing the application. Keep one
watcher running for the assigned database. It builds module changes, publishes
them, and generates client bindings. Use the supplied CLI, server URI, database
name, and module directory. See the CLI skill for command syntax.

Use the supplied server URL directly; do not register a server nickname or
change the CLI login. Use a project configuration with both publish and generate
targets. For example, in `/app/spacetime.json`, replacing the server, database,
and client directory with the supplied settings and your actual paths:

```json
{
  "server": "http://SERVER:PORT",
  "database": "DATABASE",
  "module-path": "backend/spacetimedb",
  "generate": [
    { "language": "typescript", "out-dir": "frontend/src/module_bindings" }
  ]
}
```

From `/app`, run the supplied CLI with `dev --yes --delete-data=never
--server-only`. Start the web client separately. With these configured targets,
omit `--module-path`, `--project-path`, and `--module-bindings-path` flags.
Paths in this example are relative to the project directory.
Do not run competing publish commands or watchers. If bindings generation is skipped,
correct the generate target before continuing.

Wait for the initial publish and bindings to succeed before opening the app.
After a module edit, check that the watcher published it successfully before
checking app behavior. Fix watcher errors; do not assume that the live module is current.
Restart the watcher if it exited or its configuration changed.

Keep `/app/start.sh` able to build and start the complete application from a
clean source checkout without this development session. Stop the watcher before
checking that startup path. One-shot commands remain appropriate for that script
and for diagnosing a watcher failure.
