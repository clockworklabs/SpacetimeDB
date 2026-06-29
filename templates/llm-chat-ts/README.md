Get a SpacetimeDB-backed LLM chat app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed
- An OpenRouter or OpenAI API key

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a SpacetimeDB
module and React client.

This will start the local SpacetimeDB server, publish your module, generate
TypeScript bindings, and start the React development server.

```bash
spacetime dev --template llm-chat-ts
```

## Open your app

Navigate to [http://localhost:5173](http://localhost:5173) to see your app
running.

Open the provider config modal, choose OpenRouter or OpenAI, enter an API key
and model, then start a new chat.

## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/src/index.ts` to change tables, views, reducers, and
procedures. Edit `src/App.tsx` to build the chat UI.

```
my-spacetime-app/
├── spacetimedb/              # Your SpacetimeDB module
│   └── src/
│       ├── index.ts          # Server-side tables, views, and reducers
│       └── llm.ts            # LLM provider request helpers
├── src/
│   ├── App.tsx               # React chat UI
│   └── module_bindings/      # Auto-generated types
└── package.json
```

## Understand the module

The module stores private chat threads, private chat messages, and private LLM
configuration for each SpacetimeDB identity.

The public `chat` and `message` views only expose rows owned by the connected
identity. The `llm_config` table is private, and the API key is never returned
through subscriptions or config status calls.

The API key is still stored as module data. This template is not a secret
manager: database operators can inspect module data, so use keys that are
appropriate for your local or hackathon environment.

## Configure models

Defaults:

- Provider: `openrouter`
- Model: `openai/gpt-4o-mini`
- Local database name: `llm-chat-ts`
- New chats start with a clean context.

Leaving the API key field blank keeps the saved key when editing the same
provider. Switching providers requires entering a new key.

Set `VITE_SPACETIMEDB_HOST` or `VITE_SPACETIMEDB_DB_NAME` if you publish to a
different host or database name.

## Next steps

- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
