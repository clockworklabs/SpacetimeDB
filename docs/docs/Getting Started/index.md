# Getting Started

To develop SpacetimeDB applications locally, you will need to run the Standalone version of the server.

1. [Install](/install) the SpacetimeDB CLI (Command Line Interface).
2. Run the start command

```bash
spacetime start
```

The server listens on port `3000` by default. You can change this by using the `--listen-addr` option described below.

SSL is not supported in standalone mode.

To set up your CLI to connect to the server, you can run the `spacetime server` command.

```bash
spacetime server set "http://localhost:3000"
```

## What's Next?

You are ready to start developing SpacetimeDB modules. We have a quickstart guide for each supported server-side language:

- [Rust](/docs/server-languages/rust/rust-module-quickstart-guide)
- [C#](/docs/server-languages/csharp/csharp-module-quickstart-guide)

Then you can write your client application. We have a quickstart guide for each supported client-side language:

- [Rust](/docs/client-languages/rust/rust-sdk-quickstart-guide)
- [C#](/docs/client-languages/csharp/csharp-sdk-quickstart-guide)
- [Typescript](/docs/client-languages/typescript/typescript-sdk-quickstart-guide)
- [Python](/docs/client-languages/python/python-sdk-quickstart-guide)

We also have a [step-by-step tutorial](/docs/unity-tutorial/unity-tutorial-part-1) for building a multiplayer game in Unity3d.
