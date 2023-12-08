# Getting Started

To develop SpacetimeDB applications locally, you will need to run the Standalone version of the server.

1. [Install](/install) the SpacetimeDB CLI (Command Line Interface).
2. Run the start command

```bash
spacetime start
```

The server listens on port `3000` by default. You can change this by using the `--listen-addr` option described below.

SSL is not supported in standalone mode.

## What's Next?

You are ready to start developing SpacetimeDB modules. We have a quickstart guide for each supported server-side language:

- [Rust](/docs/modules/rust/quickstart)
- [C#](/docs/modules/c-sharp/quickstart)

Then you can write your client application. We have a quickstart guide for each supported client-side language:

- [Rust](/docs/sdks/rust/quickstart)
- [C#](/docs/sdks/c-sharp/quickstart)
- [Typescript](/docs/sdks/typescript/quickstart)
- [Python](/docs/sdks/python/quickstart)

We also have a [step-by-step tutorial](/docs/unity/part-1) for building a multiplayer game in Unity3d.
