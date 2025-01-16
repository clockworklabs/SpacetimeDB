# Getting Started

To develop SpacetimeDB databases locally, you will need to run the Standalone version of the server.

1. [Install](/install) the SpacetimeDB CLI (Command Line Interface)
2. Run the start command:

```bash
spacetime start
```

The server listens on port `3000` by default, customized via `--listen-addr`.

:::rust
    ## Rust
    This is a test
:::

:::csharp
  ## C#
  This is a different test
:::

💡 Standalone mode will run in the foreground.
⚠️ SSL is not supported in standalone mode.

## What's Next?

You are ready to start developing SpacetimeDB modules. See below for a quickstart guide for both client and server (module) languages/frameworks.

### Server (Module)

- [Rust](/docs/modules/rust/quickstart)
- [C#](/docs/modules/c-sharp/quickstart)

⚡**Note:** Rust is [roughly 2x faster](https://faun.dev/c/links/faun/c-vs-rust-vs-go-a-performance-benchmarking-in-kubernetes/) than C#

### Client

- [Rust](/docs/sdks/rust/quickstart)
- [C# (Standalone)](/docs/sdks/c-sharp/quickstart)
- [C# (Unity)](/docs/unity/part-1)
- [Typescript](/docs/sdks/typescript/quickstart)
