// Pure-Node tests for the agent loop. Mocks HTTP + LoopTx.

import {
  runAgentLoop,
  buildLlmMessages,
  type LoopTx,
  type LoopMessage,
  type LoopConfig,
} from '../src/submodule/loop';
import {
  pickSummarizationCandidates,
  buildSummarizerUserContent,
  buildContextMessage,
  formatMessagesForSummarizer,
} from '../src/submodule/summarize';
import type { HttpLike } from '../src/openrouter';
import type { InvokeResult } from '../src/agent';
import { openRouterProvider } from '../src/providers';

let failures = 0;
function assert(cond: boolean, msg: string): void {
  if (!cond) {
    process.stderr.write(`  FAIL: ${msg}\n`);
    failures++;
  } else {
    process.stdout.write(`  ${msg}  OK\n`);
  }
}

const eq = (a: unknown, b: unknown): boolean =>
  JSON.stringify(a) === JSON.stringify(b);

function makeFakeStore(): {
  tx: LoopTx;
  messages: LoopMessage[];
  toolInvocations: Array<{ name: string; input: string }>;
  toolHandlers: Map<string, (input: string) => InvokeResult>;
  withTxCalls: number;
  bumpedThreads: bigint[];
  cancelledThreads: Set<bigint>;
  setTool(name: string, handler: (input: string) => InvokeResult): void;
  cancel(threadId: bigint): void;
} {
  const messages: LoopMessage[] = [];
  const toolInvocations: Array<{ name: string; input: string }> = [];
  const toolHandlers = new Map<string, (input: string) => InvokeResult>();
  const bumpedThreads: bigint[] = [];
  const cancelledThreads = new Set<bigint>();
  let nextId = 1n;

  const tx: LoopTx = {
    listMessages(threadId) {
      return messages
        .filter(m => m.threadId === threadId)
        .slice()
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    },
    appendMessage(row) {
      messages.push({ id: nextId++, attachments: [], ...row });
    },
    bumpThread(threadId) {
      bumpedThreads.push(threadId);
    },
    invokeTool(name, inputJson) {
      toolInvocations.push({ name, input: inputJson });
      const h = toolHandlers.get(name);
      if (!h) return { result: `unknown tool: ${name}`, isError: true };
      return h(inputJson);
    },
    isCancelRequested(threadId) {
      return cancelledThreads.has(threadId);
    },
  };

  return {
    tx,
    messages,
    toolInvocations,
    toolHandlers,
    bumpedThreads,
    cancelledThreads,
    withTxCalls: 0,
    setTool(name, handler) {
      toolHandlers.set(name, handler);
    },
    cancel(threadId) {
      cancelledThreads.add(threadId);
    },
  };
}

type FakeHttpResponse = { status: number; body: string } | { throws: Error };
type FakeRequestBody = {
  model?: unknown;
  messages: Array<{
    role: string;
    content?: unknown;
    tool_calls?: Array<{ id: string }>;
    tool_call_id?: string;
  }>;
  max_tokens?: unknown;
  response_format?: unknown;
  [key: string]: unknown;
};

function makeFakeHttp(responses: FakeHttpResponse[]): {
  http: HttpLike;
  requests: Array<{
    url: string;
    method: string;
    headers: Record<string, string>;
    body: FakeRequestBody;
  }>;
} {
  const requests: Array<{
    url: string;
    method: string;
    headers: Record<string, string>;
    body: FakeRequestBody;
  }> = [];
  let i = 0;
  const http: HttpLike = {
    fetch(url, init) {
      const next = responses[i++];
      requests.push({
        url,
        method: init.method,
        headers: init.headers,
        body: init.body
          ? (JSON.parse(init.body) as FakeRequestBody)
          : { messages: [] },
      });
      if (!next)
        throw new Error(`fake http: no more canned responses (call #${i})`);
      if ('throws' in next) throw next.throws;
      return { status: next.status, text: () => next.body };
    },
  };
  return { http, requests };
}

function llmReply(opts: {
  content?: string | null;
  toolCalls?: Array<{ id: string; name: string; args: object }>;
  finish?: string;
  usage?: { prompt: number; completion: number };
}): FakeHttpResponse {
  const finish =
    opts.finish ??
    (opts.toolCalls && opts.toolCalls.length > 0 ? 'tool_calls' : 'stop');
  const toolCalls = opts.toolCalls?.map(c => ({
    id: c.id,
    type: 'function',
    function: { name: c.name, arguments: JSON.stringify(c.args) },
  }));
  const u = opts.usage ?? { prompt: 10, completion: 5 };
  return {
    status: 200,
    body: JSON.stringify({
      model: 'fake/model',
      choices: [
        {
          finish_reason: finish,
          message: {
            content: opts.content ?? null,
            tool_calls: toolCalls,
          },
        },
      ],
      usage: {
        prompt_tokens: u.prompt,
        completion_tokens: u.completion,
        total_tokens: u.prompt + u.completion,
      },
    }),
  };
}

function withTxAdapter(tx: LoopTx): <R>(fn: (lt: LoopTx) => R) => R {
  return fn => fn(tx);
}

const baseCfg: LoopConfig = {
  provider: openRouterProvider,
  apiKey: 'sk-test',
  model: 'anthropic/claude-3.5-sonnet',
  systemPrompt: 'you are a test assistant',
  maxTurns: 5,
  maxHistoryMessages: 50,
  maxTokens: undefined,
  retries: 2,
  responseFormat: undefined,
  context: undefined,
};

process.stdout.write('agent loop tests\n');

// 1. Single-turn text reply
{
  const store = makeFakeStore();
  // Seed with a user message so buildLlmMessages includes it.
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([
    llmReply({ content: 'hello!', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const assistantMsgs = store.messages.filter(m => m.role === 'assistant');
  assert(
    assistantMsgs.length === 1,
    `single-turn: 1 assistant message inserted`
  );
  assert(
    assistantMsgs[0].content === 'hello!',
    `single-turn: assistant content is 'hello!'`
  );
  assert(
    assistantMsgs[0].isError === false,
    `single-turn: not flagged as error`
  );
  assert(
    assistantMsgs[0].toolCallsJson === undefined,
    `single-turn: no toolCallsJson`
  );
}

// 2. Tool call -> tool result -> follow-up text (2 turns)
{
  const store = makeFakeStore();
  store.setTool('echo', inp => {
    const args = JSON.parse(inp);
    return { result: `echoed: ${args.message}`, isError: false };
  });
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'echo hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http, requests } = makeFakeHttp([
    llmReply({
      content: '',
      toolCalls: [{ id: 'call_1', name: 'echo', args: { message: 'hi' } }],
    }),
    llmReply({ content: 'I echoed it.', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(requests.length === 2, `tool-call flow: 2 LLM calls made`);

  const inserted = store.messages.filter(m => m.id > 1n); // skip seed user msg
  assert(
    inserted.length === 3,
    `tool-call flow: assistant + tool + assistant inserted (got ${inserted.length})`
  );
  assert(
    inserted[0].role === 'assistant' && inserted[0].toolCallsJson !== undefined,
    `tool-call flow: 1st insert is assistant w/ tool_calls`
  );
  assert(
    inserted[1].role === 'tool' &&
      inserted[1].content === 'echoed: hi' &&
      inserted[1].toolCallId === 'call_1',
    `tool-call flow: 2nd insert is tool result, correctly linked to call_1`
  );
  assert(
    inserted[2].role === 'assistant' &&
      inserted[2].content === 'I echoed it.' &&
      inserted[2].toolCallsJson === undefined,
    `tool-call flow: 3rd insert is final assistant text`
  );
  assert(
    store.toolInvocations.length === 1 &&
      store.toolInvocations[0].name === 'echo',
    `tool-call flow: echo invoked once`
  );

  // 2nd LLM call must carry the assistant-with-tool_calls + tool result.
  const secondReqMsgs = requests[1].body.messages;
  const lastTwo = secondReqMsgs.slice(-2);
  assert(
    lastTwo[0].role === 'assistant' &&
      Array.isArray(lastTwo[0].tool_calls) &&
      lastTwo[0].tool_calls.length === 1,
    `tool-call flow: 2nd request includes assistant-with-tool_calls`
  );
  assert(
    lastTwo[1].role === 'tool' &&
      lastTwo[1].tool_call_id === 'call_1' &&
      lastTwo[1].content === 'echoed: hi',
    `tool-call flow: 2nd request includes tool result message`
  );
}

// 3. Multiple tool calls in one turn
{
  const store = makeFakeStore();
  store.setTool('echo', inp => ({
    result: `echoed: ${JSON.parse(inp).message}`,
    isError: false,
  }));
  store.setTool('upper', inp => ({
    result: String(JSON.parse(inp).text).toUpperCase(),
    isError: false,
  }));
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'do both',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([
    llmReply({
      content: null,
      toolCalls: [
        { id: 'a', name: 'echo', args: { message: 'one' } },
        { id: 'b', name: 'upper', args: { text: 'two' } },
      ],
    }),
    llmReply({ content: 'done', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const tools = store.messages.filter(m => m.role === 'tool');
  assert(tools.length === 2, `multi-tool: 2 tool result messages`);
  assert(
    tools[0].toolCallId === 'a' && tools[0].content === 'echoed: one',
    `multi-tool: a -> echo result`
  );
  assert(
    tools[1].toolCallId === 'b' && tools[1].content === 'TWO',
    `multi-tool: b -> upper result`
  );
  assert(store.toolInvocations.length === 2, `multi-tool: 2 invocations`);
}

// 4. HTTP error path
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([
    { status: 401, body: '{"error":"unauthorized"}' },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const assistants = store.messages.filter(m => m.role === 'assistant');
  assert(
    assistants.length === 1 && assistants[0].isError === true,
    `http-error: 1 error assistant message`
  );
  assert(
    assistants[0].content.includes('agents.provider_error:http_401'),
    `http-error: error string includes status 401`
  );
  assert(
    !assistants[0].content.includes('unauthorized'),
    `http-error: provider body is not stored in the message`
  );
}

// 5. Transport error (fetch throws every attempt)
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  // retries=2 -> 3 attempts total.
  const { http, requests } = makeFakeHttp([
    { throws: new Error('connection refused') },
    { throws: new Error('connection refused') },
    { throws: new Error('connection refused') },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(
    requests.length === 3,
    `transport-error: 3 attempts (initial + 2 retries)`
  );
  const assistants = store.messages.filter(m => m.role === 'assistant');
  assert(
    assistants.length === 1 && assistants[0].isError === true,
    `transport-error: 1 error assistant message`
  );
  assert(
    assistants[0].content.includes('agents.provider_error:transport'),
    `transport-error: error string identifies cause`
  );
}

// 6. Parse error path
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([{ status: 200, body: '{not json' }]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const assistants = store.messages.filter(m => m.role === 'assistant');
  assert(
    assistants.length === 1 &&
      assistants[0].isError === true &&
      assistants[0].content.includes('agents.provider_error:parse'),
    `parse-error: 1 error assistant message with parse kind`
  );
}

// 7. Max turns exceeded
{
  const store = makeFakeStore();
  store.setTool('loop_tool', () => ({ result: 'still going', isError: false }));
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'go',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const cfgSmall: LoopConfig = { ...baseCfg, maxTurns: 3 };
  const { http, requests } = makeFakeHttp([
    llmReply({
      content: '',
      toolCalls: [{ id: 'a', name: 'loop_tool', args: {} }],
    }),
    llmReply({
      content: '',
      toolCalls: [{ id: 'b', name: 'loop_tool', args: {} }],
    }),
    llmReply({
      content: '',
      toolCalls: [{ id: 'c', name: 'loop_tool', args: {} }],
    }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: cfgSmall,
    threadId: 1n,
  });

  assert(
    requests.length === 3,
    `max-turns: exactly maxTurns LLM calls (got ${requests.length})`
  );
  const errMsg = store.messages.find(m => m.isError === true);
  assert(
    errMsg !== undefined && errMsg.content === 'agents.max_turns_exceeded:3',
    `max-turns: final error message inserted`
  );
  assert(
    store.toolInvocations.length === 3,
    `max-turns: 3 tool invocations (one per turn)`
  );
}

// 8. Failing tool (isError=true). Loop continues if LLM asks again, ends if not.
{
  const store = makeFakeStore();
  store.setTool('flaky', () => ({ result: 'tool exploded', isError: true }));
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'try',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([
    llmReply({
      content: '',
      toolCalls: [{ id: 'x', name: 'flaky', args: {} }],
    }),
    llmReply({ content: 'sorry, that tool broke', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const toolMsg = store.messages.find(m => m.role === 'tool');
  assert(
    toolMsg !== undefined &&
      toolMsg.isError === true &&
      toolMsg.content === 'tool exploded',
    `failing-tool: tool message recorded with isError=true`
  );
  const finalAssistant = [...store.messages]
    .reverse()
    .find(m => m.role === 'assistant' && !m.isError);
  assert(
    finalAssistant !== undefined &&
      finalAssistant.content === 'sorry, that tool broke',
    `failing-tool: LLM's recovery reply recorded`
  );
}

// 9. System prompt + message history forwarded correctly
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'first',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.tx.appendMessage({
    threadId: 1n,
    role: 'assistant',
    content: 'reply',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'second',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  // Different thread; must not leak.
  store.tx.appendMessage({
    threadId: 99n,
    role: 'user',
    content: 'OTHER THREAD',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http, requests } = makeFakeHttp([
    llmReply({ content: 'ack', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const req = requests[0];
  assert(
    req.url === 'https://openrouter.ai/api/v1/chat/completions',
    `forward: posts to OpenRouter URL`
  );
  assert(
    req.headers.Authorization === 'Bearer sk-test',
    `forward: auth header set`
  );
  assert(
    req.body.model === 'anthropic/claude-3.5-sonnet',
    `forward: model included`
  );
  assert(
    req.body.messages[0].role === 'system' &&
      req.body.messages[0].content === 'you are a test assistant',
    `forward: system prompt prepended`
  );
  assert(
    eq(req.body.messages.slice(1), [
      { role: 'user', content: 'first' },
      { role: 'assistant', content: 'reply' },
      { role: 'user', content: 'second' },
    ]),
    `forward: thread-1 history in correct order, no thread-99 leak`
  );
}

// 9a. Recalled context is a leading user message, not part of the system prompt
{
  const store = makeFakeStore();
  store.messages.push({
    id: 1n,
    threadId: 1n,
    role: 'user',
    content: 'look',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
    attachments: [{ mimeType: 'image/png', data: 'AAAA' }],
  });
  const { http, requests } = makeFakeHttp([
    llmReply({ content: 'ack', finish: 'stop' }),
  ]);
  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: {
      ...baseCfg,
      context: '<conversation_context>x</conversation_context>',
    },
    threadId: 1n,
  });
  const body = requests[0].body;
  assert(
    body.messages[0].role === 'system' &&
      body.messages[0].content === 'you are a test assistant',
    `context: system prompt unchanged`
  );
  assert(
    eq(body.messages[1], {
      role: 'user',
      content: '<conversation_context>x</conversation_context>',
    }),
    `context: sent as the first user message`
  );
  assert(
    eq(body.messages[2].content, [
      { type: 'text', text: 'look' },
      { type: 'image_url', image_url: { url: 'data:image/png;base64,AAAA' } },
    ]),
    `attachments: user message carries image blocks`
  );
}

// 10. buildLlmMessages: assistant-with-tool-calls round-trips through history
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 7n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.tx.appendMessage({
    threadId: 7n,
    role: 'assistant',
    content: '',
    toolCallsJson: JSON.stringify([
      {
        id: 'c1',
        type: 'function',
        function: { name: 'echo', arguments: '{"x":1}' },
      },
    ]),
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.tx.appendMessage({
    threadId: 7n,
    role: 'tool',
    content: 'result',
    toolCallsJson: undefined,
    toolCallId: 'c1',
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const out = buildLlmMessages(store.tx, 7n, 50);
  assert(out.length === 3, `roundtrip: 3 messages built`);
  const first = out[0];
  const second = out[1];
  const third = out[2];
  assert(first?.role === 'user' && first.content === 'hi', `roundtrip: user`);
  assert(
    second?.role === 'assistant' &&
      Array.isArray(second.tool_calls) &&
      second.tool_calls[0]?.id === 'c1',
    `roundtrip: assistant carries tool_calls array reconstructed from JSON`
  );
  assert(
    third?.role === 'tool' && third.tool_call_id === 'c1',
    `roundtrip: tool message links via tool_call_id`
  );
}

// 11. Malformed toolCallsJson is dropped, not thrown
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 5n,
    role: 'assistant',
    content: 'hello',
    toolCallsJson: '{not json',
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const out = buildLlmMessages(store.tx, 5n, 50);
  const first = out[0];
  assert(
    out.length === 1 &&
      first?.role === 'assistant' &&
      first.tool_calls === undefined,
    `malformed-json: tool_calls dropped, message preserved`
  );
}

// 12. History window slides over last N messages
{
  const store = makeFakeStore();
  for (let i = 0; i < 20; i++) {
    store.tx.appendMessage({
      threadId: 1n,
      role: i % 2 === 0 ? 'user' : 'assistant',
      content: `msg-${i}`,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: false,
      promptTokens: undefined,
      completionTokens: undefined,
    });
  }
  const out = buildLlmMessages(store.tx, 1n, 5);
  assert(
    out.length === 5,
    `history-window: only last 5 messages emitted (got ${out.length})`
  );
  assert(
    out[0]?.content === 'msg-15' && out[4]?.content === 'msg-19',
    `history-window: emits the most recent slice`
  );
}

// 13. History window drops orphan tool messages (assistant turn evicted)
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 2n,
    role: 'assistant',
    content: '',
    toolCallsJson: JSON.stringify([
      {
        id: 'old',
        type: 'function',
        function: { name: 'echo', arguments: '{}' },
      },
    ]),
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.tx.appendMessage({
    threadId: 2n,
    role: 'tool',
    content: 'orphan',
    toolCallsJson: undefined,
    toolCallId: 'old',
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  // Filler so the window cuts off the assistant + tool above.
  for (let i = 0; i < 10; i++) {
    store.tx.appendMessage({
      threadId: 2n,
      role: 'user',
      content: `keep-${i}`,
      toolCallsJson: undefined,
      toolCallId: undefined,
      isError: false,
      promptTokens: undefined,
      completionTokens: undefined,
    });
  }
  const out = buildLlmMessages(store.tx, 2n, 5);
  const hasOrphan = out.some(
    m => m.role === 'tool' && m.tool_call_id === 'old'
  );
  assert(!hasOrphan, `history-window: orphan tool message dropped`);
  assert(
    out.every(m => m.role !== 'tool'),
    `history-window: only user/assistant survive`
  );
}

// 14. Tool result truncated at TOOL_RESULT_MAX
{
  const store = makeFakeStore();
  const big = 'A'.repeat(200_000);
  store.setTool('big_tool', () => ({ result: big, isError: false }));
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'go',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  const { http } = makeFakeHttp([
    llmReply({
      content: '',
      toolCalls: [{ id: 'a', name: 'big_tool', args: {} }],
    }),
    llmReply({ content: 'done', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const toolMsg = store.messages.find(m => m.role === 'tool');
  assert(toolMsg !== undefined, `tool-truncation: tool message present`);
  assert(
    toolMsg!.content.length < big.length,
    `tool-truncation: clipped (${toolMsg!.content.length} < ${big.length})`
  );
  assert(
    toolMsg!.content.endsWith('...[truncated]'),
    `tool-truncation: ends with truncation marker`
  );
}

// 15. max_tokens forwarded to OpenRouter request body
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    llmReply({ content: 'ok', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: { ...baseCfg, maxTokens: 256 },
    threadId: 1n,
  });

  assert(
    requests[0].body.max_tokens === 256,
    `max-tokens: forwarded as max_tokens=256`
  );
}

// 15a. responseFormat forwarded to the request body
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    llmReply({ content: '{}', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: { ...baseCfg, responseFormat: { type: 'json_object' } },
    threadId: 1n,
  });

  assert(
    JSON.stringify(requests[0].body.response_format) ===
      '{"type":"json_object"}',
    `response-format: forwarded as response_format=json_object`
  );
}

// 15b. responseFormat omitted when undefined
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    llmReply({ content: 'ok', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(
    !('response_format' in requests[0].body),
    `response-format-omit: field absent when undefined`
  );
}

// 16. max_tokens omitted when undefined
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    llmReply({ content: 'ok', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg, // maxTokens: undefined
    threadId: 1n,
  });

  assert(
    !('max_tokens' in requests[0].body),
    `max-tokens-omit: field absent when undefined`
  );
}

// 17. Retry succeeds after 503/503/200
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    { status: 503, body: 'unavailable' },
    { status: 503, body: 'unavailable' },
    llmReply({ content: 'finally', finish: 'stop' }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(requests.length === 3, `retry-recovery: 3 attempts made`);
  const assistant = store.messages.find(m => m.role === 'assistant');
  assert(
    assistant !== undefined &&
      !assistant.isError &&
      assistant.content === 'finally',
    `retry-recovery: success after retries`
  );
}

// 18. Retry exhausted after 3x 429
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    { status: 429, body: 'rate limited' },
    { status: 429, body: 'rate limited' },
    { status: 429, body: 'rate limited' },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(requests.length === 3, `retry-exhaust: 3 attempts then give up`);
  const errAssistant = store.messages.find(
    m => m.role === 'assistant' && m.isError
  );
  assert(
    errAssistant !== undefined &&
      errAssistant.content.includes('agents.provider_error:http_429'),
    `retry-exhaust: final error includes 429`
  );
}

// 19a. retries=0 disables retries even on 503
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    { status: 503, body: 'unavailable' },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: { ...baseCfg, retries: 0 },
    threadId: 1n,
  });

  assert(
    requests.length === 1,
    `retries=0: exactly 1 attempt on 503 (no retry)`
  );
}

// 19. No retry on non-retryable status (401)
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http, requests } = makeFakeHttp([
    { status: 401, body: 'unauthorized' },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(
    requests.length === 1,
    `no-retry-401: exactly 1 attempt (no retries on 401)`
  );
}

// 21. Usage round-trip: assistant message captures promptTokens + completionTokens
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http } = makeFakeHttp([
    llmReply({
      content: 'hi back',
      finish: 'stop',
      usage: { prompt: 47, completion: 13 },
    }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const assistant = store.messages.find(m => m.role === 'assistant');
  assert(assistant !== undefined, `usage: assistant message present`);
  assert(
    assistant!.promptTokens === 47,
    `usage: promptTokens = 47 (got ${assistant?.promptTokens})`
  );
  assert(
    assistant!.completionTokens === 13,
    `usage: completionTokens = 13 (got ${assistant?.completionTokens})`
  );
}

// 22. Usage of 0 (e.g. billing not yet reported) -> stored as undefined
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  const { http } = makeFakeHttp([
    llmReply({
      content: 'ok',
      finish: 'stop',
      usage: { prompt: 0, completion: 0 },
    }),
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const assistant = store.messages.find(m => m.role === 'assistant');
  assert(
    assistant!.promptTokens === undefined,
    `usage-zero: promptTokens=0 mapped to undefined`
  );
  assert(
    assistant!.completionTokens === undefined,
    `usage-zero: completionTokens=0 mapped to undefined`
  );
}

// 23. Tool messages and error assistants have no usage
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.setTool('echo', () => ({ result: 'r', isError: false }));
  const { http } = makeFakeHttp([
    llmReply({ toolCalls: [{ id: 'a', name: 'echo', args: {} }] }),
    { status: 401, body: 'unauthorized' },
  ]);

  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  const tool = store.messages.find(m => m.role === 'tool');
  assert(
    tool!.promptTokens === undefined && tool!.completionTokens === undefined,
    `usage: tool message has no usage`
  );
  const err = store.messages.find(m => m.role === 'assistant' && m.isError);
  assert(
    err!.promptTokens === undefined && err!.completionTokens === undefined,
    `usage: error assistant has no usage`
  );
}

// 24. Cancel before turn 1: loop bails immediately, no LLM call
{
  const store = makeFakeStore();
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'hi',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });
  store.cancel(1n);

  const { http, requests } = makeFakeHttp([]);
  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });
  assert(requests.length === 0, `cancel-before: 0 LLM calls`);
  const cancelMsg = store.messages.find(
    m => m.role === 'assistant' && m.content === 'agents.cancelled'
  );
  assert(
    cancelMsg !== undefined && cancelMsg.isError === true,
    `cancel-before: cancelled message inserted`
  );
}

// 25. Cancel between turns: loop completes turn 1, sees cancel, bails before turn 2
{
  const store = makeFakeStore();
  store.setTool('echo', () => ({ result: 'r', isError: false }));
  store.tx.appendMessage({
    threadId: 1n,
    role: 'user',
    content: 'go',
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
  });

  // Trigger cancel inside the appendMessage hook of turn 1.
  let triggeredCancel = false;
  const originalAppend = store.tx.appendMessage.bind(store.tx);
  store.tx.appendMessage = row => {
    originalAppend(row);
    if (!triggeredCancel && row.role === 'assistant' && row.toolCallsJson) {
      triggeredCancel = true;
      store.cancel(1n);
    }
  };

  const { http, requests } = makeFakeHttp([
    llmReply({ content: '', toolCalls: [{ id: 'a', name: 'echo', args: {} }] }),
  ]);
  runAgentLoop({
    http,
    withTx: withTxAdapter(store.tx),
    llmToolDefs: [],
    cfg: baseCfg,
    threadId: 1n,
  });

  assert(
    requests.length === 1,
    `cancel-between: only 1 LLM call (turn 2 skipped)`
  );
  const cancelMsg = store.messages.find(
    m => m.role === 'assistant' && m.content === 'agents.cancelled'
  );
  assert(
    cancelMsg !== undefined && cancelMsg.isError === true,
    `cancel-between: cancelled message inserted on turn 2 entry`
  );
  assert(
    store.toolInvocations.length === 1,
    `cancel-between: turn 1's tool call completed before cancel`
  );
}

process.stdout.write('\nsummarization helper tests\n');

function mkMsg(
  id: bigint,
  role: string,
  content: string,
  extras: Partial<LoopMessage> = {}
): LoopMessage {
  return {
    id,
    threadId: 1n,
    role,
    content,
    toolCallsJson: undefined,
    toolCallId: undefined,
    isError: false,
    promptTokens: undefined,
    completionTokens: undefined,
    attachments: [],
    ...extras,
  };
}

// 26. pickSummarizationCandidates: history fits window -> null
{
  const messages = [mkMsg(1n, 'user', 'a'), mkMsg(2n, 'assistant', 'b')];
  assert(
    pickSummarizationCandidates(messages, 5, null) === null,
    `pickCandidates: history within window -> null`
  );
}

// 27. pickSummarizationCandidates: 10 messages, window=4 -> 6 dropped
{
  const messages = Array.from({ length: 10 }, (_, i) =>
    mkMsg(BigInt(i + 1), 'user', `m${i}`)
  );
  const result = pickSummarizationCandidates(messages, 4, null);
  assert(
    result !== null && result.newDropped.length === 6,
    `pickCandidates: 10 msgs, window=4 -> 6 dropped`
  );
  assert(
    result!.lastNewId === 6n,
    `pickCandidates: lastNewId is the last dropped id`
  );
}

// 28. pickSummarizationCandidates: respects summarizedThroughId
{
  // 10 msgs, window=4 -> ids 1-6 dropped; summary covers through 4 -> only 5,6 new.
  const messages = Array.from({ length: 10 }, (_, i) =>
    mkMsg(BigInt(i + 1), 'user', `m${i}`)
  );
  const result = pickSummarizationCandidates(messages, 4, 4n);
  assert(
    result !== null && result.newDropped.length === 2,
    `pickCandidates: respects summarizedThroughId -> only NEW dropped (got ${result?.newDropped.length})`
  );
  assert(
    result!.newDropped[0].id === 5n && result!.lastNewId === 6n,
    `pickCandidates: newDropped starts at first uncovered id`
  );
}

// 29. pickSummarizationCandidates: summary already covers all dropped -> null
{
  const messages = Array.from({ length: 10 }, (_, i) =>
    mkMsg(BigInt(i + 1), 'user', `m${i}`)
  );
  // summary covers 1-7 (>= window-cutoff at 6) -> nothing new
  assert(
    pickSummarizationCandidates(messages, 4, 7n) === null,
    `pickCandidates: summary covers all dropped -> null`
  );
}

// 30. formatMessagesForSummarizer: roles render distinctly
{
  const m = [
    mkMsg(1n, 'user', 'hello'),
    mkMsg(2n, 'assistant', 'hi back'),
    mkMsg(3n, 'tool', 'tool_result_text', { toolCallId: 'c1' }),
  ];
  const out = formatMessagesForSummarizer(m);
  assert(out.includes('User: hello'), `format: user line`);
  assert(out.includes('Assistant: hi back'), `format: assistant line`);
  assert(
    out.includes('[Tool result: tool_result_text]'),
    `format: tool result line`
  );
}

// 31. formatMessagesForSummarizer: assistant tool_calls render
{
  const m = [
    mkMsg(1n, 'assistant', '', {
      toolCallsJson: JSON.stringify([
        { function: { name: 'echo', arguments: '{"x":1}' } },
      ]),
    }),
  ];
  const out = formatMessagesForSummarizer(m);
  assert(
    out.includes('[Assistant called tool echo({"x":1})]'),
    `format: assistant tool call rendered`
  );
}

// 32. buildSummarizerUserContent: with existing summary
{
  const m = [mkMsg(1n, 'user', 'hi')];
  const out = buildSummarizerUserContent('prior summary text', m);
  assert(
    out.includes('Existing summary:\nprior summary text'),
    `buildContent: includes existing summary`
  );
  assert(
    out.includes('Additional messages'),
    `buildContent: asks for extension when summary exists`
  );
}

// 33. buildSummarizerUserContent: without existing summary
{
  const m = [mkMsg(1n, 'user', 'hi')];
  const out = buildSummarizerUserContent(null, m);
  assert(
    !out.includes('Existing summary'),
    `buildContent: no existing summary header`
  );
  assert(
    out.includes('Messages to summarize'),
    `buildContent: from-scratch header`
  );
}

// 34. buildContextMessage: nothing recalled -> no message
{
  assert(
    buildContextMessage(undefined, []) === undefined,
    `context: no summary and no snippets -> undefined`
  );
  assert(
    buildContextMessage('', []) === undefined,
    `context: empty summary -> undefined`
  );
}

// 35. buildContextMessage: summary and snippets are delimited reference data
{
  const out = buildContextMessage('we discussed APIs', ['[user] hello']);
  assert(
    out !== undefined &&
      out.startsWith('<conversation_context>') &&
      out.endsWith('</conversation_context>'),
    `context: wrapped in delimiters`
  );
  assert(out!.includes('we discussed APIs'), `context: includes summary`);
  assert(out!.includes('[user] hello'), `context: includes snippets`);
  assert(out!.includes('not instructions'), `context: marked as data`);
}

if (failures > 0) {
  process.stderr.write(`\n${failures} test(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nall agent loop tests passed.\n');
