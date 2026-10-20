import assert from 'node:assert/strict';
import test from 'node:test';

import { brokerProtocol } from '../container/broker-protocols.js';
import { CODING_PROVIDERS } from '../container/coding-providers.js';
import { ANTIGRAVITY_SUBAGENT_TOOLS, ANTIGRAVITY_TOOLS, ANTIGRAVITY_WEB_TOOLS } from '../src/agents/antigravity-protocol.js';
import { DEFAULT_TOOL_POLICY, toolPolicyFromEnvironment, unavailableToolsReason } from '../src/agents/tool-policy.js';
import type { ToolPolicy } from '../src/agents/tool-policy.js';
import { extensionAttemptKey } from '../src/campaigns/campaign-extension.js';
import type { CampaignAttemptPlan } from '../src/campaigns/campaign-compiler.js';

const POLICIES: ToolPolicy[] = [
  { webSearch: true, subagents: false }, { webSearch: false, subagents: false },
  { webSearch: true, subagents: true }, { webSearch: false, subagents: true },
];

test('the default tool policy turns web search and subagents off, as every agent ran in 1.1', () => {
  assert.deepEqual(DEFAULT_TOOL_POLICY, { webSearch: false, subagents: false });
  assert.deepEqual(toolPolicyFromEnvironment({}), DEFAULT_TOOL_POLICY);
  assert.deepEqual(toolPolicyFromEnvironment({ STACK_BENCH_WEB_SEARCH: 'off', STACK_BENCH_SUBAGENTS: 'on' }),
    { webSearch: false, subagents: true });
  assert.throws(() => toolPolicyFromEnvironment({ STACK_BENCH_WEB_SEARCH: 'true' }), /STACK_BENCH_WEB_SEARCH/);
});

test('Claude Code keeps --bare and refuses a policy that turns on tools --bare does not have', () => {
  const args = (tools: ToolPolicy) => CODING_PROVIDERS.anthropic.args({ model: 'claude-test', effort: 'high',
    baseUrl: 'http://broker', resumeSession: null, maxBudgetUsd: null, tools });
  const off = args({ webSearch: false, subagents: false });
  assert(off.includes('--bare'));
  assert(!off.some(arg => /tools/i.test(arg)), 'the 1.1 arguments, with no tool flags');
  for (const tools of POLICIES.filter(policy => policy.webSearch || policy.subagents)) {
    assert.throws(() => args(tools), /cannot be on/);
  }
  assert.match(unavailableToolsReason('anthropic', { webSearch: true, subagents: false })!, /webSearch.*--bare/);
  for (const provider of ['openai', 'xai', 'google', 'openrouter', null]) {
    assert.equal(unavailableToolsReason(provider, { webSearch: true, subagents: false }), null);
  }
});

test('subagents are refused for every agent until the audit is shown to see their work', () => {
  for (const provider of ['anthropic', 'openai', 'xai', 'google', 'openrouter', null]) {
    for (const webSearch of [false, true]) {
      assert.match(unavailableToolsReason(provider, { webSearch, subagents: true })!, /subagents.*audit/);
    }
  }
});

test('an extension matches a parent planned before campaigns recorded a tool policy', () => {
  const attempt = { stack: 'postgres', model: 'gpt-test', agentAdapter: 'codex', repetition: 1,
    condition: { id: 'neutral', guidance: { mode: 'neutral' }, repair: null } } as unknown as CampaignAttemptPlan;
  assert.equal(extensionAttemptKey(attempt), extensionAttemptKey({ ...attempt, tools: { ...DEFAULT_TOOL_POLICY } }));
  assert.notEqual(extensionAttemptKey(attempt), extensionAttemptKey({ ...attempt, tools: { webSearch: true, subagents: false } }));
});

test('the broker passes Antigravity its web and subagent functions only when the policy turns them on', () => {
  const declared = [...ANTIGRAVITY_TOOLS, ...ANTIGRAVITY_WEB_TOOLS, ...ANTIGRAVITY_SUBAGENT_TOOLS, 'ask_question',
    'browser_subagent', 'schedule_message'];
  for (const tools of [undefined, ...POLICIES]) {
    const protocol = brokerProtocol({ provider: 'google', mode: 'api-key', credential: 'x'.repeat(16),
      sessionToken: 'y'.repeat(16), model: 'test-model', maxOutputTokens: 4096, ...(tools ? { tools } : {}) });
    const body = Buffer.from(JSON.stringify({ contents: [], tools: [{ functionDeclarations: declared.map(name => ({ name })) }] }));
    const forwarded = protocol.parseRequest(body, '/v1beta/models/test-model:streamGenerateContent') as {
      tools: Array<{ functionDeclarations: Array<{ name: string }> }> };
    assert.deepEqual(forwarded.tools[0]!.functionDeclarations.map(declaration => declaration.name), [...ANTIGRAVITY_TOOLS,
      ...(tools?.webSearch ? ANTIGRAVITY_WEB_TOOLS : []), ...(tools?.subagents ? ANTIGRAVITY_SUBAGENT_TOOLS : [])]);
    // Google's own search and URL tools are web search too.
    const hosted = Buffer.from(JSON.stringify({ contents: [], tools: [{ googleSearch: {} }, { urlContext: {} },
      { functionDeclarations: [{ name: 'view_file' }], googleSearchRetrieval: {} }] }));
    const sent = (protocol.parseRequest(hosted, '/v1beta/models/test-model:streamGenerateContent') as {
      tools: Array<Record<string, unknown>> }).tools;
    assert.deepEqual(sent, tools?.webSearch
      ? [{ googleSearch: {} }, { urlContext: {} }, { functionDeclarations: [{ name: 'view_file' }], googleSearchRetrieval: {} }]
      : [{ functionDeclarations: [{ name: 'view_file' }] }], JSON.stringify(tools));
  }
});

test('a declared web search is priced when unused, and each search is counted when used', () => {
  const base = { mode: 'api-key' as const, credential: 'x'.repeat(16), sessionToken: 'y'.repeat(16), model: 'test-model',
    maxOutputTokens: 4096 };
  const sse = (events: unknown[]) => Buffer.from(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(''));

  const anthropic = brokerProtocol(base);
  assert.deepEqual(anthropic.requestPricing({ tools: [{ type: 'web_search_20250305', name: 'web_search' }] }),
    { unpriced: null, requiresUsage: 'server-tool', bounded: false });
  const message = (requests: number) => Buffer.from(JSON.stringify({ usage: { input_tokens: 1, output_tokens: 1,
    server_tool_use: { web_search_requests: requests } } }));
  assert.equal(anthropic.responseUsage(message(3))?.web_search_calls, 3);
  assert.equal(anthropic.responseUsage(message(3))?.unpriced_reason, 'server-tool');
  assert.equal(anthropic.responseUsage(message(0))?.unpriced_reason, null);

  const usage = { input_tokens: 10, output_tokens: 2, input_tokens_details: { cached_tokens: 0 } };
  for (const provider of ['openai', 'xai'] as const) {
    const responses = brokerProtocol({ ...base, provider });
    assert.deepEqual(responses.requestPricing({ tools: [{ type: 'function', name: 'shell' }, { type: 'web_search' }] }),
      { unpriced: null, requiresUsage: 'hosted-tool', bounded: false }, provider);
    assert.equal(responses.requestPricing({ tools: [{ type: 'file_search' }] }).unpriced, 'hosted-tool');
    const searched = responses.responseUsage(sse([
      { type: 'response.output_item.done', item: { id: 'ws_1', type: 'web_search_call', status: 'completed' } },
      { type: 'response.output_item.done', item: { id: 'ws_2', type: 'web_search_call', status: 'completed' } },
      { type: 'response.completed', response: { usage, output: [{ id: 'ws_1', type: 'web_search_call' },
        { id: 'ws_2', type: 'web_search_call' }, { id: 'msg_1', type: 'message' }] } },
    ]));
    assert.equal(searched?.web_search_calls, 2, provider);
    assert.equal(searched?.unpriced_reason, 'hosted-tool', provider);
    const unused = responses.responseUsage(sse([{ type: 'response.completed', response: { usage, output: [] } }]));
    assert.equal(unused?.web_search_calls, 0, provider);
    assert.equal(unused?.unpriced_reason, null, provider);
  }

  const gemini = brokerProtocol({ ...base, provider: 'google' });
  assert.deepEqual(gemini.requestPricing({ tools: [{ googleSearch: {} }] }),
    { unpriced: null, requiresUsage: 'hosted-tool', bounded: false });
  assert.equal(gemini.requestPricing({ tools: [{ codeExecution: {} }] }).unpriced, 'hosted-tool');
  const grounded = gemini.responseUsage(sse([
    { candidates: [{ groundingMetadata: { webSearchQueries: ['spacetimedb views', 'spacetimedb procedures'] } }] },
    { usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 2 } },
  ]));
  assert.equal(grounded?.web_search_calls, 2);
  assert.equal(grounded?.unpriced_reason, 'hosted-tool');
  assert.equal(gemini.responseUsage(sse([{ usageMetadata: { promptTokenCount: 10, candidatesTokenCount: 2 } }]))
    ?.unpriced_reason, null);
});
