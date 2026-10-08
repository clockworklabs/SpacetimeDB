# Agents chat example

Chat with an AI model, keep separate conversations, and try a tool that
returns the server's current time.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

From `spacetime-agents-ts/example`, copy [.env.example](./.env.example) to `.env`.

You also need an OpenRouter API key for the default agents. Set
`OPENROUTER_API_KEY` in `.env`. Model requests can use paid credits.
Setting only an OpenAI or Anthropic key does not change the default provider.

Then publish the example and start its web server:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://localhost:8789>.

## Try it

1. Create an account and click **New chat**.
2. Send a message and wait for a reply.
3. Ask the agent to use its tool to get the current server time.
4. Reload the page. Your conversation should still be there.
5. Sign in with a different account in a private browser window. That account
   has its own conversations.

The default agents send requests through OpenRouter, including models named
OpenAI or Anthropic.

## Change the provider or agent

Edit the definitions in [spacetimedb/src/agents/](./spacetimedb/src/agents/).
To use OpenAI or Anthropic directly, set the agent's `defaultProvider` and
choose models that provider accepts. Set the matching `OPENAI_API_KEY` or
`ANTHROPIC_API_KEY` in `.env`. Check the summarizer's definition too.

Tools live in [spacetimedb/src/tools/](./spacetimedb/src/tools/).
The chat agent's `tools` setting controls which tools it can call.

## Configuration

See [.env.example](./.env.example) for provider keys and optional sign-in settings.

- Set both `RATE_LIMIT_TOKENS_PER_WINDOW` and `RATE_LIMIT_WINDOW_SECS` to
  limit each user's token usage.
- Keep `AUTH_ISSUER_URL` and `AUTH_BASE_URL` set to the address you open
  in the browser. Use `localhost` consistently for the default setup.

## Before deploying

Set usage limits and control who can create accounts before exposing a paid
model to the public.

## Troubleshooting

- **No API key or provider error:** the key must match the agent's provider.
  Choosing a different model alone does not change that provider.
- **Startup configuration fails:** use the CLI account that published the database.
- **Sign-in fails after a database reset:** clear this site's browser data and
  create an account again.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts): chat requests and account checks.
- [spacetimedb/src/agents/](./spacetimedb/src/agents/): agent settings.
- [public/ui.js](./public/ui.js): chat controls.

After changing server code, run `pnpm run build:module`. Restart
`pnpm run dev` after changing browser code or `.env`.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in
the local `spacetime-agents-example` database.**

To use the submodule in your own app, see the
[Agents quick start](../README.md#quick-start).
