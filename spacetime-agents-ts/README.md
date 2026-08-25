# @spacetimedb/agents

Typed tools, agent definitions, chat-provider adapters, embeddings helpers, and
dispatch utilities for SpacetimeDB TypeScript modules. This pure helper leaves
persistence, authorization, and lifecycle hooks to the host module.

## Install

```bash
npm install @spacetimedb/agents spacetimedb@^2.8.3
```

`spacetimedb` is a peer dependency. Keep its version aligned with the SDK used
to build the host module.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

This helper package supplies agent and provider primitives. Your host module
owns conversation tables, authorization, provider-key storage, and the
procedure that performs HTTP. Define the registry at module scope, then call
the provider from a procedure with `ctx.http`.

Define tools with SpacetimeDB type builders. The declaration produces the JSON
Schema sent to the model and validates every returned tool call before the
handler runs.

```ts
import { t } from 'spacetimedb/server';
import {
  agentTool,
  callChat,
  defineAgent,
  makeAgentRegistry,
  openRouterProvider,
} from '@spacetimedb/agents';

const getTime = agentTool('Return the current module time.', t.unit(), ctx =>
  String(ctx.timestamp.microsSinceUnixEpoch)
);

const agents = {
  support: defineAgent({
    defaultProvider: 'openrouter',
    defaultModel: 'openai/gpt-4o-mini',
    defaultSystemPrompt: 'Answer concisely.',
    tools: { get_time: getTime },
  }),
};

const registry = makeAgentRegistry(agents);
const definition = registry.agentDef('support');
if (!definition) throw new Error('unknown agent');

const result = callChat(ctx.http, openRouterProvider, {
  apiKey,
  model: definition.defaultModel,
  system: definition.defaultSystemPrompt,
  messages: [{ role: 'user', content: 'What time is it?' }],
  tools: registry.llmToolDefsFor('support'),
  retries: definition.defaultRetries,
});
```

Run model calls from a procedure or HTTP handler. Reducers remain deterministic.
Keep API keys in private tables and pass the stored value to `callChat` at the
call site.

The snippet uses application-owned `ctx` and `apiKey` values inside that
procedure. See the complete
[Agents host module](./example/spacetimedb/)
for private configuration, caller-scoped views, and an agent loop.

## API

- `agentTool(description, args, run)` defines a typed tool.
- `defineAgent(config)` applies defaults to an agent definition.
- `makeAgentDispatch(tools)` builds tool definitions and an invocation method.
- `makeAgentRegistry(agents)` selects agents and dispatches their tools.
- `typeBuilderToJsonSchema(typeBuilder)` converts supported tool arguments.
- `callChat(http, provider, request)` performs one synchronous chat request,
  with optional immediate retries for retryable failures.
- `openRouterProvider`, `openAiProvider`, and `anthropicProvider` adapt their
  providers' chat APIs.
- `openAiEmbeddingsProvider` and `openRouterEmbeddingsProvider` perform
  embedding requests.
- `cosineSimilarity` and `topKByScore` provide in-memory ranking helpers.

Documented subpath exports are `./kit`, `./openrouter`, `./providers`,
`./embeddings`, and `./stale-locks`.

Tool dispatch rejects malformed JSON, missing and unknown fields, incorrect
types, unsafe integers, inputs above 64 KiB, arrays above 1,000 items, and tool
results above 64 KiB. Agent definitions validate turns, history, token, retry,
and RAG limits during initialization.

### Application boundary

Export a host procedure that loads a private provider key, calls the selected
agent, and returns an application-specific result. After generating client
bindings, that procedure is called like any other SpacetimeDB procedure:

```ts
const answer = await conn.procedures.askSupport({
  message: 'How do I update my billing address?',
});
```

`askSupport` belongs to the host module. Its implementation should authorize
the caller, load the API key from a private table, pass `ctx.http` to
`callChat`, and map provider errors to stable application errors. Conversation
rows should be written through `ctx.withTx` and exposed through caller-scoped
views.

Package entrypoints:

- `@spacetimedb/agents` exports the complete public surface.
- `@spacetimedb/agents/kit` exports typed agents, tools, and dispatch.
- `@spacetimedb/agents/providers` exports provider adapters.
- `@spacetimedb/agents/embeddings` exports embedding and ranking helpers.
- `@spacetimedb/agents/openrouter` exports the common chat request layer.
- `@spacetimedb/agents/stale-locks` exports the bounded stale-lock cleanup
  helpers used by the reference host module.

## Testing

```bash
npm test --workspace @spacetimedb/agents
npm run lint --workspace @spacetimedb/agents
```

The unit suite uses mocked HTTP with deterministic provider fixtures. See the
[complete example](./example/)
for a host module and client.

## License

BUSL-1.1. See [`LICENSE.txt`](./LICENSE.txt).
