// Pure-Node tests for the Agents package.
// Avoids importing 'spacetimedb/server' (Node 22 ESM can't parse its `using` decls);
// builds minimal AlgebraicType fixtures matching what t.object(...) would produce.

import {
  agentTool,
  makeAgentDispatch,
  defineAgent,
  makeAgentRegistry,
  typeBuilderToJsonSchema,
} from '../src/agent.ts';
import {
  openRouterProvider,
  openAiProvider,
  anthropicProvider,
} from '../src/providers.ts';
import {
  callChat,
  type HttpLike,
  type ChatRequest,
  type ToolDefinition,
} from '../src/openrouter.ts';
import {
  cosineSimilarity,
  topKByScore,
  openAiEmbeddingsProvider,
  openRouterEmbeddingsProvider,
} from '../src/embeddings.ts';
import {
  deleteStaleThreadLocks,
  staleLockCutoffMicros,
} from '../src/stale-locks.ts';

type AT = { tag: string; value?: unknown };

import type { AlgebraicType } from 'spacetimedb';
import type { TypeBuilder } from 'spacetimedb/server';

const fake = <T = unknown>(at: AT): TypeBuilder<T, AlgebraicType> =>
  ({ algebraicType: at }) as unknown as TypeBuilder<T, AlgebraicType>;

const _bool = (): AT => ({ tag: 'Bool' });
const _string = (): AT => ({ tag: 'String' });
const _i8 = (): AT => ({ tag: 'I8' });
const _u8 = (): AT => ({ tag: 'U8' });
const _i16 = (): AT => ({ tag: 'I16' });
const _u16 = (): AT => ({ tag: 'U16' });
const _i32 = (): AT => ({ tag: 'I32' });
const _u32 = (): AT => ({ tag: 'U32' });
const _i64 = (): AT => ({ tag: 'I64' });
const _u64 = (): AT => ({ tag: 'U64' });
const _u128 = (): AT => ({ tag: 'U128' });
const _f64 = (): AT => ({ tag: 'F64' });
const _array = (e: AT): AT => ({ tag: 'Array', value: e });
const _object = (props: Record<string, AT>): AT => ({
  tag: 'Product',
  value: {
    elements: Object.entries(props).map(([name, at]) => ({
      name,
      algebraicType: at,
    })),
  },
});
const _unit = (): AT => ({ tag: 'Product', value: { elements: [] } });
const _option = (inner: AT): AT => ({
  tag: 'Sum',
  value: {
    variants: [
      { name: 'some', algebraicType: inner },
      { name: 'none', algebraicType: _unit() },
    ],
  },
});

let failures = 0;
function assert(cond: boolean, msg: string): void {
  if (!cond) {
    process.stderr.write(`  FAIL: ${msg}\n`);
    failures++;
  } else {
    process.stdout.write(`  ${msg}  OK\n`);
  }
}

process.stdout.write('\nstale lock sweep tests\n');

{
  const nowMicros = 20_000n;
  const cutoffMicros = staleLockCutoffMicros(nowMicros, 1_000n);
  const locks = Array.from({ length: 600 }, (_, id) => ({
    id,
    lockedAt: { microsSinceUnixEpoch: nowMicros },
  }));
  locks.push({ id: 600, lockedAt: { microsSinceUnixEpoch: 1_000n } });

  const expiredIndexRows = locks
    .filter(lock => lock.lockedAt.microsSinceUnixEpoch < cutoffMicros)
    .sort((a, b) =>
      a.lockedAt.microsSinceUnixEpoch < b.lockedAt.microsSinceUnixEpoch ? -1 : 1
    );
  const deleted = new Set<number>();
  const count = deleteStaleThreadLocks(expiredIndexRows, cutoffMicros, lock =>
    deleted.add(lock.id)
  );

  assert(count === 1, 'sweep reaches an expired lock after 600 fresh inserts');
  assert(deleted.has(600), 'sweep deletes the expired lock');
  assert(
    [...deleted].every(id => id === 600),
    'sweep preserves every fresh lock'
  );
}

const eq = (a: unknown, b: unknown): boolean =>
  JSON.stringify(a) === JSON.stringify(b);

process.stdout.write('typeBuilderToJsonSchema tests\n');

// 1. unit -> empty object schema
{
  const schema = typeBuilderToJsonSchema(fake(_unit()));
  assert(
    eq(schema, { type: 'object', properties: {} }),
    `unit() -> empty object`
  );
}

// 2. object with required primitives
{
  const tb = fake(
    _object({
      name: _string(),
      count: _i32(),
      ratio: _f64(),
      on: _bool(),
    })
  );
  const schema = typeBuilderToJsonSchema(tb);
  assert(
    eq(schema, {
      type: 'object',
      properties: {
        name: { type: 'string' },
        count: { type: 'integer' },
        ratio: { type: 'number' },
        on: { type: 'boolean' },
      },
      required: ['name', 'count', 'ratio', 'on'],
    }),
    `object with primitives -> all required`
  );
}

// 3. option fields excluded from required, unwrapped to inner schema
{
  const tb = fake(
    _object({
      must: _string(),
      maybe: _option(_string()),
    })
  );
  const schema = typeBuilderToJsonSchema(tb);
  assert(
    schema.required !== undefined &&
      schema.required.includes('must') &&
      !schema.required.includes('maybe'),
    `option fields excluded from required`
  );
  assert(
    eq(schema.properties.maybe, { type: 'string' }),
    `option<string> unwraps to plain string schema`
  );
}

// 4. nested object
{
  const tb = fake(
    _object({
      inner: _object({ a: _i64() }),
    })
  );
  const schema = typeBuilderToJsonSchema(tb);
  assert(
    eq(schema.properties.inner, {
      type: 'object',
      properties: { a: { type: 'integer' } },
      required: ['a'],
    }),
    `nested object inlined recursively`
  );
}

// 5. array of strings
{
  const tb = fake(_object({ tags: _array(_string()) }));
  const schema = typeBuilderToJsonSchema(tb);
  assert(
    eq(schema.properties.tags, { type: 'array', items: { type: 'string' } }),
    `array<string> -> {type:array, items:{type:string}}`
  );
}

// 6. all i*/u* through 64 bits roll up to integer
{
  const tb = fake(
    _object({
      a: _i8(),
      b: _u8(),
      c: _i16(),
      d: _u16(),
      e: _i32(),
      f: _u32(),
      g: _i64(),
      h: _u64(),
    })
  );
  const schema = typeBuilderToJsonSchema(tb);
  const allInt = Object.values(schema.properties).every(
    v =>
      typeof v === 'object' &&
      v !== null &&
      (v as { type?: unknown }).type === 'integer'
  );
  assert(allInt, `all i*/u* up to 64 bits -> integer`);
}

// 7. 128/256-bit integers throw (not JSON-representable)
{
  let threw = false;
  try {
    typeBuilderToJsonSchema(fake(_object({ x: _u128() })));
  } catch (err) {
    threw = err instanceof Error && err.message.includes('U128');
  }
  assert(threw, `u128 field rejected with helpful error`);
}

// 8. non-product top-level rejected
{
  let threw = false;
  try {
    typeBuilderToJsonSchema(fake(_string()));
  } catch (err) {
    threw = err instanceof Error && err.message.includes('object');
  }
  assert(threw, `top-level non-object rejected`);
}

// 9. true sum (non-option) -> oneOf
{
  const sumAt: AT = {
    tag: 'Sum',
    value: {
      variants: [
        { name: 'a', algebraicType: _string() },
        { name: 'b', algebraicType: _i32() },
        { name: 'c', algebraicType: _bool() },
      ],
    },
  };
  const tb = fake(_object({ kind: sumAt }));
  const schema = typeBuilderToJsonSchema(tb);
  const kind = schema.properties.kind as { oneOf?: unknown[] };
  const oneOf = Array.isArray(kind.oneOf) ? kind.oneOf : [];
  assert(oneOf.length === 3, `3-variant sum -> oneOf with 3 entries`);
  assert(
    eq(oneOf[0], {
      type: 'object',
      properties: {
        tag: { type: 'string', enum: ['a'] },
        value: { type: 'string' },
      },
      required: ['tag'],
    }),
    `sum variant shape: {tag, value}`
  );
}

process.stdout.write('\nagentTool + makeAgentDispatch tests\n');

// 10. agentTool retains TypeBuilder algebraicType
{
  const tool = agentTool(
    'echoes the message back',
    fake<{ msg: string }>(_object({ msg: _string() })),
    (_ctx, args) => `echo: ${args.msg}`
  );
  const at = tool.algebraicType;
  assert(at?.tag === 'Product', `agentTool retains algebraicType`);
}

// 11. dispatch builds llmToolDefs in expected shape + invoke paths
{
  const echo = agentTool(
    'echoes the message back',
    fake<{ msg: string }>(_object({ msg: _string() })),
    (_ctx, args) => `echo: ${args.msg}`
  );
  const noop = agentTool(
    'does nothing, takes no args',
    fake(_unit()),
    _ctx => 'ok'
  );
  const { llmToolDefs, invoke } = makeAgentDispatch<
    unknown,
    { echo: typeof echo; noop: typeof noop }
  >({ echo, noop });

  assert(llmToolDefs.length === 2, `2 tool defs emitted`);
  assert(
    llmToolDefs[0].function.name === 'echo' &&
      llmToolDefs[0].function.description === 'echoes the message back',
    `echo tool def name + description`
  );
  assert(
    eq(llmToolDefs[0].function.parameters, {
      type: 'object',
      properties: { msg: { type: 'string' } },
      required: ['msg'],
    }),
    `echo tool def parameters schema`
  );
  assert(
    eq(llmToolDefs[1].function.parameters, { type: 'object', properties: {} }),
    `unit-arg tool def has empty properties`
  );

  const r1 = invoke({}, 'echo', JSON.stringify({ msg: 'hi' }));
  assert(r1.isError === false && r1.result === 'echo: hi', `invoke echo OK`);

  const r2 = invoke({}, 'noop', '');
  assert(
    r2.isError === false && r2.result === 'ok',
    `invoke noop with empty input string`
  );

  const r3 = invoke({}, 'nope', '{}');
  assert(
    r3.isError === true && r3.result.includes('unknown tool'),
    `unknown tool returns isError`
  );

  const r4 = invoke({}, 'echo', '{not json');
  assert(
    r4.isError === true && r4.result.includes('invalid JSON'),
    `bad JSON returns isError`
  );

  const boom = agentTool('throws', fake(_unit()), _ctx => {
    throw new Error('kaboom');
  });
  const d2 = makeAgentDispatch<unknown, { boom: typeof boom }>({ boom });
  const r5 = d2.invoke({}, 'boom', '');
  assert(
    r5.isError === true && r5.result === 'kaboom',
    `tool throw becomes isError result`
  );

  const missing = invoke({}, 'echo', '{}');
  assert(
    missing.isError === true && missing.result.includes('msg is required'),
    `missing required field rejected before handler`
  );
  const wrongType = invoke({}, 'echo', '{"msg":42}');
  assert(
    wrongType.isError === true && wrongType.result.includes('must be a string'),
    `wrong field type rejected before handler`
  );
  const unknown = invoke({}, 'echo', '{"msg":"hi","extra":true}');
  assert(
    unknown.isError === true && unknown.result.includes('extra is not allowed'),
    `unknown field rejected before handler`
  );
  const oversized = invoke(
    {},
    'echo',
    JSON.stringify({ msg: 'x'.repeat(70_000) })
  );
  assert(
    oversized.isError === true && oversized.result.includes('exceeds'),
    `oversized tool input rejected before parsing`
  );
}

// 12. invalid tool name rejected at dispatch construction
{
  const bad = agentTool('x', fake(_unit()), () => 'ok');
  let threw = false;
  try {
    makeAgentDispatch<unknown, Record<string, typeof bad>>({
      'has spaces': bad,
    });
  } catch (err) {
    threw = err instanceof Error && err.message.includes('must match');
  }
  assert(threw, `invalid tool name 'has spaces' rejected`);
}

// 13. valid tool names accepted (a-z, A-Z, 0-9, _, -)
{
  const ok = agentTool('x', fake(_unit()), () => 'ok');
  let threw = false;
  try {
    makeAgentDispatch<unknown, Record<string, typeof ok>>({
      send_message: ok,
      'get-time': ok,
      tool42: ok,
    });
  } catch {
    threw = true;
  }
  assert(!threw, `valid tool names accepted`);
}

// 14. Prototype-key lookup reported as 'unknown tool', not dispatched.
{
  const ok = agentTool('x', fake(_unit()), () => 'ok');
  const { invoke } = makeAgentDispatch<unknown, Record<string, typeof ok>>({
    real: ok,
  });
  for (const name of [
    'toString',
    'constructor',
    '__proto__',
    'hasOwnProperty',
  ]) {
    const r = invoke({}, name, '{}');
    assert(
      r.isError === true && r.result === `unknown tool: ${name}`,
      `prototype-key '${name}' rejected as unknown tool`
    );
  }
}

process.stdout.write('\ndefineAgent + makeAgentRegistry tests\n');

// 15. defineAgent fills in defaults for omitted optional fields
{
  const echo = agentTool(
    'echo back',
    fake<{ message: string }>(_object({ message: _string() })),
    (_ctx, args) => `echo: ${args.message}`
  );
  const a = defineAgent({
    defaultModel: 'm/x',
    tools: { echo },
  });
  assert(a.defaultModel === 'm/x', `defineAgent: defaultModel set`);
  assert(
    a.defaultMaxTurns === 10,
    `defineAgent: defaultMaxTurns falls back to 10`
  );
  assert(
    a.defaultMaxHistoryMessages === 50,
    `defineAgent: defaultMaxHistoryMessages falls back to 50`
  );
  assert(a.defaultRetries === 2, `defineAgent: defaultRetries falls back to 2`);
  assert(
    a.defaultSystemPrompt === undefined,
    `defineAgent: systemPrompt remains undefined when omitted`
  );
  assert(
    a.defaultMaxTokens === undefined,
    `defineAgent: maxTokens remains undefined when omitted`
  );
  assert(
    a.defaultResponseFormat === undefined,
    `defineAgent: responseFormat remains undefined when omitted`
  );
}

// 15b. Invalid agent limits fail during definition.
{
  let threw = false;
  try {
    defineAgent({ defaultModel: 'm/x', defaultMaxTurns: 0, tools: {} });
  } catch (err) {
    threw = err instanceof Error && err.message.includes('defaultMaxTurns');
  }
  assert(threw, `defineAgent rejects an invalid turn limit`);
}

// 16. makeAgentRegistry exposes names() / has() / agentDef()
{
  const echo = agentTool(
    'echo back',
    fake<{ message: string }>(_object({ message: _string() })),
    (_ctx, args) => `echo: ${args.message}`
  );
  const noop = agentTool('does nothing', fake(_unit()), _ctx => 'ok');
  const chat = defineAgent({ defaultModel: 'm/chat', tools: { echo, noop } });
  const summary = defineAgent({
    defaultModel: 'm/summary',
    defaultMaxTurns: 1,
    defaultResponseFormat: { type: 'json_object' },
    tools: {},
  });
  const reg = makeAgentRegistry<
    unknown,
    { chat: typeof chat; summary: typeof summary }
  >({ chat, summary });

  assert(
    eq(reg.names().sort(), ['chat', 'summary']),
    `registry: names() returns all agent names`
  );
  assert(reg.has('chat') === true, `registry: has('chat') = true`);
  assert(
    reg.has('does-not-exist') === false,
    `registry: has('does-not-exist') = false`
  );
  assert(
    reg.agentDef('chat')?.defaultModel === 'm/chat',
    `registry: agentDef returns the right def`
  );
  assert(
    reg.agentDef('summary')?.defaultResponseFormat !== undefined,
    `registry: per-agent responseFormat preserved`
  );
}

// 17. Registry routes tool dispatch by agent name
{
  const echo = agentTool(
    'echo',
    fake<{ msg: string }>(_object({ msg: _string() })),
    (_ctx, args) => `chat-echo: ${args.msg}`
  );
  const otherEcho = agentTool(
    'echo',
    fake<{ msg: string }>(_object({ msg: _string() })),
    (_ctx, args) => `summary-echo: ${args.msg}`
  );
  const chat = defineAgent({ defaultModel: 'm/chat', tools: { echo } });
  const summary = defineAgent({
    defaultModel: 'm/sum',
    tools: { echo: otherEcho },
  });
  const reg = makeAgentRegistry<
    unknown,
    { chat: typeof chat; summary: typeof summary }
  >({ chat, summary });

  const r1 = reg.invoke('chat', {}, 'echo', JSON.stringify({ msg: 'hi' }));
  assert(
    r1.isError === false && r1.result === 'chat-echo: hi',
    `registry: invoke('chat', echo) hits chat's tool`
  );
  const r2 = reg.invoke('summary', {}, 'echo', JSON.stringify({ msg: 'hi' }));
  assert(
    r2.isError === false && r2.result === 'summary-echo: hi',
    `registry: invoke('summary', echo) hits summary's tool (same name, different agent)`
  );
  const r3 = reg.invoke('does-not-exist', {}, 'echo', '{}');
  assert(
    r3.isError === true && r3.result.includes('unknown agent'),
    `registry: invoke on unknown agent returns isError`
  );
  const r4 = reg.invoke('summary', {}, 'echo_typo', '{}');
  assert(
    r4.isError === true && r4.result.includes('unknown tool'),
    `registry: invoke with unknown tool name in valid agent returns isError`
  );
}

// 18. Per-agent llmToolDefs differs by agent
{
  const echo = agentTool(
    'echo',
    fake<{ msg: string }>(_object({ msg: _string() })),
    (_ctx, args) => `e: ${args.msg}`
  );
  const chat = defineAgent({ defaultModel: 'm/chat', tools: { echo } });
  const summary = defineAgent({ defaultModel: 'm/sum', tools: {} });
  const reg = makeAgentRegistry<
    unknown,
    { chat: typeof chat; summary: typeof summary }
  >({ chat, summary });

  assert(reg.llmToolDefsFor('chat').length === 1, `tool-defs: chat has 1 tool`);
  assert(
    reg.llmToolDefsFor('summary').length === 0,
    `tool-defs: summary has 0 tools`
  );
  assert(
    reg.llmToolDefsFor('does-not-exist').length === 0,
    `tool-defs: unknown agent returns empty list`
  );
}

// 19. Invalid agent name rejected at registry construction
{
  const a = defineAgent({ defaultModel: 'm/x', tools: {} });
  let threw = false;
  try {
    makeAgentRegistry<unknown, Record<string, typeof a>>({ 'has spaces': a });
  } catch (err) {
    threw = err instanceof Error && err.message.includes('must match');
  }
  assert(threw, `agent-name: invalid name rejected`);
}

// Provider adapters

process.stdout.write('\nprovider adapter tests\n');

const sampleTools: ToolDefinition[] = [
  {
    type: 'function',
    function: {
      name: 'echo',
      description: 'echo back',
      parameters: {
        type: 'object',
        properties: { msg: { type: 'string' } },
        required: ['msg'],
      },
    },
  },
];

const baseReq: ChatRequest = {
  apiKey: 'sk-test',
  model: 'some/model',
  system: 'you are helpful',
  messages: [{ role: 'user', content: 'hi' }],
  tools: sampleTools,
  maxTokens: 256,
};

// 20. openRouterProvider builds OpenAI-shape body, posts to OpenRouter URL.
{
  const { url, headers, body } = openRouterProvider.buildRequest(baseReq);
  assert(
    url === 'https://openrouter.ai/api/v1/chat/completions',
    `openrouter: OpenRouter URL`
  );
  assert(
    headers['Authorization'] === 'Bearer sk-test',
    `openrouter: Bearer auth`
  );
  const b = JSON.parse(body);
  assert(b.model === 'some/model', `openrouter: model carried`);
  assert(
    b.messages[0].role === 'system' &&
      b.messages[0].content === 'you are helpful',
    `openrouter: system prepended as first message`
  );
  assert(b.messages[1].role === 'user', `openrouter: user follows system`);
  assert(
    Array.isArray(b.tools) && b.tools[0].function.name === 'echo',
    `openrouter: tools in OpenAI shape`
  );
  assert(b.tool_choice === 'auto', `openrouter: tool_choice='auto'`);
  assert(b.max_tokens === 256, `openrouter: max_tokens carried`);
}

// 21. openAiProvider differs only in URL (same wire format).
{
  const { url, headers } = openAiProvider.buildRequest(baseReq);
  assert(
    url === 'https://api.openai.com/v1/chat/completions',
    `openai: native OpenAI URL`
  );
  assert(headers['Authorization'] === 'Bearer sk-test', `openai: Bearer auth`);
}

// 22. anthropicProvider translates: system separate, tools renamed, max_tokens required.
{
  const { url, headers, body } = anthropicProvider.buildRequest(baseReq);
  assert(
    url === 'https://api.anthropic.com/v1/messages',
    `anthropic: messages URL`
  );
  assert(headers['x-api-key'] === 'sk-test', `anthropic: x-api-key auth`);
  assert(
    headers['anthropic-version'] === '2023-06-01',
    `anthropic: version header`
  );
  const b = JSON.parse(body);
  assert(
    b.system === 'you are helpful',
    `anthropic: system as top-level field`
  );
  assert(
    b.messages.length === 1 && b.messages[0].role === 'user',
    `anthropic: system NOT in messages array`
  );
  assert(b.max_tokens === 256, `anthropic: max_tokens carried`);
  assert(
    b.tools[0].name === 'echo' && b.tools[0].input_schema !== undefined,
    `anthropic: tools renamed (function.name -> name, parameters -> input_schema)`
  );
  assert(
    eq(b.tool_choice, { type: 'auto' }),
    `anthropic: tool_choice is object {type:'auto'}`
  );
}

// 23. anthropicProvider: tool_calls in assistant messages become content blocks.
{
  const req: ChatRequest = {
    apiKey: 'sk-test',
    model: 'claude-3-5-sonnet',
    messages: [
      { role: 'user', content: 'do it' },
      {
        role: 'assistant',
        content: '',
        tool_calls: [
          {
            id: 'tc_1',
            type: 'function',
            function: { name: 'echo', arguments: '{"msg":"hi"}' },
          },
        ],
      },
      { role: 'tool', tool_call_id: 'tc_1', content: 'echoed' },
    ],
  };
  const { body } = anthropicProvider.buildRequest(req);
  const b = JSON.parse(body);
  assert(b.messages.length === 3, `anthropic: 3 messages`);
  const asst = b.messages[1];
  assert(
    asst.role === 'assistant' && Array.isArray(asst.content),
    `anthropic: assistant content is content-block array`
  );
  assert(
    asst.content.some(
      (block: unknown) =>
        typeof block === 'object' &&
        block !== null &&
        (block as { type?: unknown }).type === 'tool_use' &&
        (block as { id?: unknown }).id === 'tc_1'
    ),
    `anthropic: tool_call -> tool_use block`
  );
  const tr = b.messages[2];
  assert(
    tr.role === 'user' &&
      tr.content[0].type === 'tool_result' &&
      tr.content[0].tool_use_id === 'tc_1',
    `anthropic: tool result wrapped in user/tool_result block`
  );
}

// 24. anthropicProvider: default max_tokens when caller omits.
{
  const req: ChatRequest = {
    apiKey: 'k',
    model: 'm',
    messages: [{ role: 'user', content: 'x' }],
  };
  const { body } = anthropicProvider.buildRequest(req);
  assert(
    JSON.parse(body).max_tokens > 0,
    `anthropic: max_tokens always present`
  );
}

// 25. parseResponse: OpenAI shape.
{
  const r = openRouterProvider.parseResponse(
    JSON.stringify({
      model: 'gpt-4',
      choices: [
        {
          finish_reason: 'stop',
          message: { content: 'hello back' },
        },
      ],
      usage: { prompt_tokens: 12, completion_tokens: 5, total_tokens: 17 },
    }),
    'some/model'
  );
  assert(
    r.text === 'hello back' && r.finishReason === 'stop',
    `parse-openai: text + finishReason`
  );
  assert(
    r.usage.promptTokens === 12 && r.usage.completionTokens === 5,
    `parse-openai: usage`
  );
  assert(r.toolCalls.length === 0, `parse-openai: no tool calls`);
}

// 26. parseResponse: OpenAI tool calls.
{
  const r = openRouterProvider.parseResponse(
    JSON.stringify({
      model: 'gpt-4',
      choices: [
        {
          finish_reason: 'tool_calls',
          message: {
            content: null,
            tool_calls: [
              {
                id: 'tc_a',
                type: 'function',
                function: { name: 'echo', arguments: '{"msg":"hi"}' },
              },
            ],
          },
        },
      ],
      usage: {},
    }),
    'some/model'
  );
  assert(
    r.text === null &&
      r.toolCalls.length === 1 &&
      r.toolCalls[0].function.name === 'echo',
    `parse-openai: tool_calls extracted`
  );
  assert(r.finishReason === 'tool_calls', `parse-openai: tool_calls finish`);
}

// 27. parseResponse: Anthropic content blocks + stop_reason mapping.
{
  const r = anthropicProvider.parseResponse(
    JSON.stringify({
      model: 'claude-3-5-sonnet',
      stop_reason: 'end_turn',
      content: [{ type: 'text', text: 'hello back' }],
      usage: { input_tokens: 12, output_tokens: 5 },
    }),
    'claude-3-5-sonnet'
  );
  assert(r.text === 'hello back', `parse-anthropic: text extracted`);
  assert(r.finishReason === 'stop', `parse-anthropic: end_turn -> stop`);
  assert(
    r.usage.promptTokens === 12 && r.usage.completionTokens === 5,
    `parse-anthropic: usage mapped (input_tokens -> promptTokens)`
  );
  assert(r.usage.totalTokens === 17, `parse-anthropic: total computed`);
}

// 27b. Malformed provider usage cannot produce NaN or negative totals.
{
  const openAi = openAiProvider.parseResponse(
    JSON.stringify({
      choices: [{ finish_reason: 'stop', message: { content: 'ok' } }],
      usage: {
        prompt_tokens: 'oops',
        completion_tokens: -5,
        total_tokens: null,
      },
    }),
    'm'
  );
  assert(
    eq(openAi.usage, { promptTokens: 0, completionTokens: 0, totalTokens: 0 }),
    `parse-openai: malformed usage normalizes to zero`
  );
}

// 28. parseResponse: Anthropic tool_use blocks -> normalized tool calls.
{
  const r = anthropicProvider.parseResponse(
    JSON.stringify({
      model: 'claude-3-5-sonnet',
      stop_reason: 'tool_use',
      content: [
        { type: 'text', text: 'using tool' },
        { type: 'tool_use', id: 'tu_1', name: 'echo', input: { msg: 'hi' } },
      ],
      usage: {},
    }),
    'claude-3-5-sonnet'
  );
  assert(r.text === 'using tool', `parse-anthropic: text from text block`);
  assert(
    r.finishReason === 'tool_calls',
    `parse-anthropic: tool_use -> tool_calls`
  );
  assert(
    r.toolCalls.length === 1 && r.toolCalls[0].function.name === 'echo',
    `parse-anthropic: tool_use -> toolCalls`
  );
  assert(
    r.toolCalls[0].function.arguments === '{"msg":"hi"}',
    `parse-anthropic: input -> JSON-stringified arguments`
  );
}

// 29. callChat dispatches through chosen provider.
{
  const http: HttpLike = {
    fetch(url, init) {
      // Verify Anthropic URL came through.
      assert(
        url === 'https://api.anthropic.com/v1/messages',
        `callChat: routes to provider URL`
      );
      assert(
        (init.headers as Record<string, string>)['x-api-key'] === 'sk-test',
        `callChat: provider headers`
      );
      return {
        status: 200,
        text: () =>
          JSON.stringify({
            model: 'claude-3-5-sonnet',
            stop_reason: 'end_turn',
            content: [{ type: 'text', text: 'ok' }],
            usage: { input_tokens: 1, output_tokens: 1 },
          }),
      };
    },
  };
  const result = callChat(http, anthropicProvider, {
    apiKey: 'sk-test',
    model: 'claude-3-5-sonnet',
    messages: [{ role: 'user', content: 'hi' }],
  });
  assert(
    result.ok && result.response.text === 'ok',
    `callChat: returns parsed response from chosen provider`
  );
}

// 30. defineAgent: provider defaults to 'openrouter'.
{
  const a = defineAgent({ defaultModel: 'm', tools: {} });
  assert(
    a.defaultProvider === 'openrouter',
    `defineAgent: default provider is openrouter`
  );
}

// 31. defineAgent: explicit provider preserved.
{
  const a = defineAgent({
    defaultProvider: 'anthropic',
    defaultModel: 'm',
    tools: {},
  });
  assert(
    a.defaultProvider === 'anthropic',
    `defineAgent: explicit provider preserved`
  );
}

// Embeddings + RAG helpers

process.stdout.write('\nembeddings + RAG tests\n');

// 32. Cosine: identical vectors -> 1.0
{
  const v = [1, 2, 3];
  assert(Math.abs(cosineSimilarity(v, v) - 1) < 1e-9, `cosine: identical -> 1`);
}

// 33. Cosine: orthogonal -> 0
{
  assert(
    Math.abs(cosineSimilarity([1, 0], [0, 1])) < 1e-9,
    `cosine: orthogonal -> 0`
  );
}

// 34. Cosine: anti-parallel -> -1
{
  assert(
    Math.abs(cosineSimilarity([1, 2, 3], [-1, -2, -3]) - -1) < 1e-9,
    `cosine: anti-parallel -> -1`
  );
}

// 35. Cosine: zero vector -> 0 (no NaN)
{
  assert(
    cosineSimilarity([0, 0, 0], [1, 2, 3]) === 0,
    `cosine: zero vector returns 0`
  );
}

// 36. Cosine: length mismatch -> 0
{
  assert(
    cosineSimilarity([1, 2], [1, 2, 3]) === 0,
    `cosine: length mismatch -> 0`
  );
}

// 37. topKByScore picks the highest-scoring items in descending order
{
  const items = ['a', 'b', 'c', 'd'];
  const scores: Record<string, number> = { a: 0.1, b: 0.9, c: 0.5, d: 0.7 };
  const top = topKByScore(items, x => scores[x], 2);
  assert(top.length === 2, `topK: count`);
  assert(top[0].item === 'b' && top[1].item === 'd', `topK: descending`);
  assert(top[0].score === 0.9, `topK: score carried`);
}

// 38. topKByScore k=0 returns empty
{
  assert(topKByScore([1, 2, 3], x => x, 0).length === 0, `topK: k=0 -> empty`);
}

// 39. topKByScore k > length returns all
{
  const out = topKByScore(['x', 'y'], () => 1, 10);
  assert(out.length === 2, `topK: k > length returns all`);
}

// 40. openAiEmbeddingsProvider builds correct request + parses response.
{
  let captured: { url?: string; body?: unknown } = {};
  const http: HttpLike = {
    fetch(url, init) {
      captured = { url, body: JSON.parse(init.body!) };
      return {
        status: 200,
        text: () =>
          JSON.stringify({
            model: 'text-embedding-3-small',
            data: [
              { embedding: [0.1, 0.2, 0.3] },
              { embedding: [0.4, 0.5, 0.6] },
            ],
            usage: { prompt_tokens: 8, total_tokens: 8 },
          }),
      };
    },
  };
  const r = openAiEmbeddingsProvider.embed(
    http,
    'sk-test',
    'text-embedding-3-small',
    ['hello', 'world']
  );
  const capturedBody = captured.body as Record<string, unknown>;
  assert(
    captured.url === 'https://api.openai.com/v1/embeddings',
    `embeddings: openai URL`
  );
  assert(
    capturedBody.model === 'text-embedding-3-small',
    `embeddings: model carried`
  );
  assert(
    eq(capturedBody.input, ['hello', 'world']),
    `embeddings: inputs array`
  );
  assert(
    r.ok && r.vectors.length === 2 && eq(r.vectors[0], [0.1, 0.2, 0.3]),
    `embeddings: vectors parsed in order`
  );
  assert(r.ok && r.usage.promptTokens === 8, `embeddings: usage parsed`);
}

// 41. openRouterEmbeddingsProvider hits OpenRouter URL.
{
  let capturedUrl = '';
  const http: HttpLike = {
    fetch(url) {
      capturedUrl = url;
      return {
        status: 200,
        text: () =>
          JSON.stringify({
            model: 'm',
            data: [{ embedding: [1, 2] }],
            usage: {},
          }),
      };
    },
  };
  openRouterEmbeddingsProvider.embed(http, 'k', 'm', ['t']);
  assert(
    capturedUrl === 'https://openrouter.ai/api/v1/embeddings',
    `embeddings: openrouter URL`
  );
}

// Multimodal content blocks

// 42a. openRouter/openAi serialize image attachments as image_url data URIs.
{
  const req: ChatRequest = {
    apiKey: 'k',
    model: 'gpt-4o',
    messages: [
      {
        role: 'user',
        content: [
          { type: 'text', text: 'caption this' },
          { type: 'image', mimeType: 'image/png', data: 'BASE64DATA' },
        ],
      },
    ],
  };
  const { body } = openAiProvider.buildRequest(req);
  const b = JSON.parse(body);
  assert(
    Array.isArray(b.messages[0].content),
    `multimodal-openai: content is array`
  );
  assert(
    b.messages[0].content[0].type === 'text' &&
      b.messages[0].content[0].text === 'caption this',
    `multimodal-openai: text block`
  );
  assert(
    b.messages[0].content[1].type === 'image_url',
    `multimodal-openai: image becomes image_url block`
  );
  assert(
    b.messages[0].content[1].image_url.url ===
      'data:image/png;base64,BASE64DATA',
    `multimodal-openai: data URI built correctly`
  );
}

// 42b. Anthropic serializes image attachments as { type:'image', source:{base64,media_type,data} }.
{
  const req: ChatRequest = {
    apiKey: 'k',
    model: 'claude-3-5-sonnet',
    messages: [
      {
        role: 'user',
        content: [
          { type: 'text', text: 'caption this' },
          { type: 'image', mimeType: 'image/jpeg', data: 'BASE64DATA' },
        ],
      },
    ],
  };
  const { body } = anthropicProvider.buildRequest(req);
  const b = JSON.parse(body);
  assert(
    Array.isArray(b.messages[0].content),
    `multimodal-anthropic: content is array`
  );
  assert(
    b.messages[0].content[1].type === 'image',
    `multimodal-anthropic: image block type='image'`
  );
  assert(
    eq(b.messages[0].content[1].source, {
      type: 'base64',
      media_type: 'image/jpeg',
      data: 'BASE64DATA',
    }),
    `multimodal-anthropic: source structured correctly`
  );
}

// 42c. String content unchanged through providers (backward compat).
{
  const req: ChatRequest = {
    apiKey: 'k',
    model: 'm',
    messages: [{ role: 'user', content: 'plain text' }],
  };
  const oa = JSON.parse(openAiProvider.buildRequest(req).body);
  const an = JSON.parse(anthropicProvider.buildRequest(req).body);
  assert(
    oa.messages[0].content === 'plain text',
    `compat: openai keeps string content`
  );
  assert(
    an.messages[0].content === 'plain text',
    `compat: anthropic keeps string content`
  );
}

// 42. Embedding HTTP errors return parseable error shape.
{
  const http: HttpLike = {
    fetch: () => ({ status: 401, text: () => '{"error":"unauthorized"}' }),
  };
  const r = openAiEmbeddingsProvider.embed(http, 'bad', 'm', ['x']);
  assert(
    !r.ok && r.error.kind === 'http' && r.error.status === 401,
    `embeddings: http error parsed`
  );
}

if (failures > 0) {
  process.stderr.write(`\n${failures} test(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nall Agents tests passed.\n');
