---
title: Getting Started
slug: /getting-started
---

To develop SpacetimeDB databases locally, you will need to run the Standalone version of the server.

1. [Install](https://spacetimedb.com/install) the SpacetimeDB CLI (Command Line Interface)
2. Run the start command:

```bash
spacetime start
```

The server listens on port `3000` by default, customized via `--listen-addr`.

💡 Standalone mode will run in the foreground.
⚠️ SSL is not supported in standalone mode.

## What's Next?

### Log in to SpacetimeDB

SpacetimeDB authenticates users using a GitHub login, to prevent unauthorized access (e.g. somebody else publishing over your module). Log in to SpacetimeDB using:

```bash
spacetime login
```

This will open a browser and ask you to log in via GitHub. If you forget this step, any commands that require login (like `spacetime publish`) will ask you to log in when you run them.

You are now ready to start developing SpacetimeDB modules. See below for a quickstart guide for both client and server (module) languages/frameworks.

### Server (Module)

- [Rust](/modules/rust/quickstart)
- [C#](/modules/c-sharp/quickstart)

⚡**Note:** Rust is [roughly 2x faster](https://faun.dev/c/links/faun/c-vs-rust-vs-go-a-performance-benchmarking-in-kubernetes/) than C#

### Client

- [Rust](/sdks/rust/quickstart)
- [C# (Standalone)](/sdks/c-sharp/quickstart)
- [C# (Unity)](/unity/part-1)
- [Typescript](/sdks/typescript/quickstart)
