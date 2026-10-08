# @spacetimedb/agents

Build AI assistants in your SpacetimeDB application. Store conversations, send
messages to a model, and give the assistant tools that read or update your
application's data.

Choose which models and tools each assistant can use. Your application controls
who can access each conversation and how much they can spend on model calls.

## Install

```bash
npm install @spacetimedb/agents spacetimedb
```

`spacetimedb` is a peer dependency. Keep its version aligned with the SDK used
to build the host module.

## Quick start

This example creates a chat agent with a tool that returns the server's current
time. Users can start conversations, send messages, and read their conversations.

Configure a provider API key before sending a message. Add your application's
quota check in `beforeRun` before allowing users to make paid model calls.

```ts
import {
  schema,
  t,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as agents from '@spacetimedb/agents/submodule';
import { agentTool, defineAgent } from '@spacetimedb/agents';

const spacetimedb = schema({ agents });
export default spacetimedb;
type Tx = TransactionCtx<InferSchema<typeof spacetimedb>>;

export const init = spacetimedb.init(ctx => {
  agents.install(ctx.as.agents);
});

// Tools receive the host transaction.
const getTime = agentTool('Return the current module time.', t.unit(), ctx =>
  String((ctx as Tx).timestamp.microsSinceUnixEpoch)
);

const agentsClient = agents.client<Tx>({
  agents: {
    support: defineAgent({
      defaultModel: 'openai/gpt-4o-mini',
      models: ['openai/gpt-4o-mini', 'anthropic/claude-haiku-4.5'],
      defaultSystemPrompt: 'Answer concisely.',
      tools: { get_time: getTime },
    }),
  },
  submodule: tx => tx.as.agents,
  beforeRun: (tx, run) => {
    // Throw to reject the run, for example when run.owner is over quota.
  },
});

export const startThread = spacetimedb.procedure(
  { agentName: t.string() },
  t.u64(),
  (ctx, { agentName }) => {
    const owner = ctx.sender.toHexString();
    return ctx.withTx(tx => agentsClient.startThread(tx, { owner, agentName }));
  }
);

export const sendMessage = spacetimedb.procedure(
  { threadId: t.u64(), content: t.string() },
  t.unit(),
  (ctx, { threadId, content }) => {
    const owner = ctx.sender.toHexString();
    agentsClient.sendMessage(ctx, { owner, threadId, content });
    return {};
  }
);

export const myThreads = spacetimedb.view(
  { name: 'my_threads', public: true },
  t.array(agents.thread.rowType),
  ctx => [...ctx.db.agents.thread.owner.filter(ctx.sender.toHexString())]
);
```

`install` makes the installing identity the first Agents administrator.
An administrator sets the provider key after publishing, for example
`spacetime call <db> agents.set_api_key
'"openrouter"' '"<key>"'`.

See the [complete example](./example/) for sign-in, attachments, and a token
quota.

## Client

`client(config)` validates the agent definitions and returns the thread and
model-run operations. Config fields:

- `agents`: agent definitions by runtime name. Threads store the name.
- `submodule(tx)`: returns the Agents submodule context of a host transaction,
  normally `tx => tx.as.agents`.
- `beforeRun(tx, { owner, threadId, agentName })`: runs in the transaction
  that starts each model run, including title generation. Throw to reject it.
- `onUsage(tx, { owner, threadId, agentName, promptTokens, completionTokens })`:
  receives the token usage of each model response.
- `attachments(tx, messageId)`: returns base64 image attachments for a user
  message.
- `maxThreadsPerOwner`: defaults to 100.

Operations take an `owner` key chosen by the host, such as an identity or an
authenticated user id. Each operation checks that the thread belongs to that
owner.

- `startThread(tx, { owner, agentName, title?, metadata? })` returns the thread
  id.
- `updateThread(tx, { owner, threadId, title?, modelOverride?, metadata?,
clearTitle?, clearModelOverride?, clearMetadata? })`. `modelOverride` must be
  listed in the agent's `models`.
- `deleteThread(tx, { owner, threadId })` and
  `requestCancel(tx, { owner, threadId })`.
- `sendMessage(ctx, { owner, threadId, content, onInsert? })` stores the user
  message and runs the agent loop. `onInsert(tx, messageId)` runs in the same
  transaction, for example to store attachments; empty content requires it.
- `regenerateResponse(ctx, { owner, threadId })` deletes the replies after the
  last user message and runs the agent again.
- `generateThreadTitle(ctx, { owner, threadId })` titles an untitled thread.
- `status(tx)` returns each agent's effective model and selectable models, the
  configured providers, and the stale-lock threshold.
- `isAdmin(tx)` reports whether the sender is an Agents administrator.

`tx` is a host transaction context and `ctx` is a host procedure context. Model
calls need `ctx.http`, so `sendMessage`, `regenerateResponse`, and
`generateThreadTitle` run from procedures.

The client does not authorize callers. Every host operation must authenticate
the caller, derive `owner` from trusted state, and apply a quota through
`beforeRun` before it spends provider credits. Messages per thread are bounded
only by that quota.

## Agent behavior

Effective settings resolve in this order:

1. Operator rows set with `agents.set_agent_override`.
2. A thread's `modelOverride`, when the agent lists it in `models`.
3. The agent definition.

Users cannot change system prompts. Running summaries and retrieved earlier
messages are sent as a delimited user message, never as part of the system
prompt. Provider error details are written to the module log; the stored
assistant message contains only `agents.provider_error:<kind>`.

Thread locks prevent two runs on one thread. A lock is released when the run
ends, and the scheduled sweeper removes locks older than the configured
threshold.

## Administration

Namespaced reducers, callable by Agents administrators:

- `set_api_key(provider, key)` and `clear_api_key(provider)` for the built-in
  `openrouter`, `openai`, and `anthropic` providers.
- `set_agent_override(agentName, provider?, model?, systemPrompt?, maxTurns?,
maxHistoryMessages?, maxTokens?, retries?)` and
  `clear_agent_override(agentName)`. Overrides for names that no client agent
  defines are stored but unused.
- `set_agent_config(staleLockThresholdSecs?)`.
- `add_agent_admin_identity(identity)` and
  `remove_agent_admin_identity(identity)`.
- `clear_thread_lock(threadId)`.

All submodule tables are private.

## Lower-level API

Use these when the application needs its own conversation tables or procedure
boundary.

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
- `errors` lists the error codes the package throws.

Tool dispatch rejects malformed JSON, missing and unknown fields, incorrect
types, unsafe integers, inputs above 64 KiB, arrays above 1,000 items, and tool
results above 64 KiB. Agent definitions validate turns, history, token, retry,
and RAG limits during initialization.

Tools run inside the host transaction, so a tool can read the host's tables.

Package entrypoints:

- `@spacetimedb/agents` exports the lower-level API and `errors`.
- `@spacetimedb/agents/submodule` exports the Agents schema, `install`,
  `client`, `errors`, the administration reducers, and the `thread`,
  `message`, and `threadLock` tables for host views.
- `@spacetimedb/agents/providers` exports provider adapters.
- `@spacetimedb/agents/embeddings` exports embedding and ranking helpers.
- `@spacetimedb/agents/openrouter` exports the common chat request layer.

## Testing

```bash
pnpm test
pnpm run lint
```

Unit tests use mocked provider responses.

## License

Apache-2.0.
