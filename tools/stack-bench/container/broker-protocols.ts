import type { IncomingMessage, OutgoingHttpHeaders } from 'node:http';
import { brotliDecompressSync, gunzipSync, inflateSync } from 'node:zlib';
import { createParser } from 'eventsource-parser';
import type { BrokerConfig } from './credential-broker-accounting.js';

const MAX_REQUEST_BYTES = 32 * 1024 * 1024;
type JsonRecord = Record<string, unknown>;
function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function isNumber(value: unknown): value is number { return typeof value === 'number'; }
function fail(message: string): never { throw new Error(`credential broker: ${message}`); }

function upstreamHeaders(request: IncomingMessage, config: BrokerConfig): OutgoingHttpHeaders {
  const headers: OutgoingHttpHeaders = { ...request.headers };
  delete headers.host;
  // Request identity encoding so accounting and the client read the same bytes.
  delete headers['accept-encoding'];
  delete headers.authorization;
  delete headers['proxy-authorization'];
  delete headers['x-api-key'];
  if (config.mode === 'api-key') headers['x-api-key'] = config.credential;
  else headers.authorization = `Bearer ${config.credential}`;
  return headers;
}

function parseProviderRequest(body: Buffer, path: string, config: BrokerConfig): JsonRecord {
  let payload: unknown;
  try { payload = JSON.parse(body.toString('utf8')); }
  catch { fail('request body must be valid JSON'); }
  if (!isRecord(payload)) {
    fail('request body must be an object');
  }
  if (payload.model !== config.model) fail('request model does not match the selected model');
  if (path === '/v1/messages'
    && (!isNumber(payload.max_tokens) || !Number.isInteger(payload.max_tokens) || payload.max_tokens < 1
      || payload.max_tokens > config.maxOutputTokens)) {
    fail(`max_tokens must be from 1 through ${config.maxOutputTokens}`);
  }
  return payload;
}

function decodedResponseBody(body: Buffer, contentEncoding: string | string[] | undefined): Buffer {
  const encodings = String(contentEncoding ?? '')
    .split(',').map(value => value.trim().toLowerCase()).filter(Boolean);
  let decoded = body;
  for (const encoding of encodings.reverse()) {
    if (encoding === 'identity') continue;
    const options = { maxOutputLength: MAX_REQUEST_BYTES };
    if (encoding === 'gzip' || encoding === 'x-gzip') decoded = gunzipSync(decoded, options);
    else if (encoding === 'deflate') decoded = inflateSync(decoded, options);
    else if (encoding === 'br') decoded = brotliDecompressSync(decoded, options);
    else throw new Error(`unsupported response encoding ${encoding}`);
  }
  return decoded;
}

function responseUsage(body: Buffer, contentEncoding: string | string[] | undefined = undefined): JsonRecord | null {
  const values: JsonRecord[] = [];
  const add = (value: unknown): void => {
    if (!isRecord(value)) return;
    if (isRecord(value.usage)) values.push(value.usage);
    if (isRecord(value.message) && isRecord(value.message.usage)) values.push(value.message.usage);
  };
  let text: string;
  try { text = decodedResponseBody(body, contentEncoding).toString('utf8'); }
  catch { return null; }
  try {
    add(JSON.parse(text));
  } catch {
    let sawError = false;
    let sawFinalUsage = false;
    let sawMessageStop = false;
    let parseError = false;
    const parser = createParser({
      maxBufferSize: MAX_REQUEST_BYTES,
      onError: () => { parseError = true; },
      onEvent: ({ data }) => {
        if (!data || data === '[DONE]') return;
        try {
          const event = JSON.parse(data);
          if (isRecord(event) && event.type === 'error') sawError = true;
          if (isRecord(event) && event.type === 'message_delta' && isRecord(event.usage)) {
            sawFinalUsage = true;
          }
          if (isRecord(event) && event.type === 'message_stop') sawMessageStop = true;
          add(event);
        } catch { /* Ignore non-JSON event data. */ }
      },
    });
    try { parser.feed(`${text}\n\n`); }
    catch { parseError = true; }
    if (parseError || sawError || !sawFinalUsage || !sawMessageStop) return null;
  }
  if (values.length === 0) return null;
  const number = (field: string): number => Math.max(0, ...values.map(value => Number(value[field]) || 0));
  const cacheWrite = (field: string): number => Math.max(0, ...values.map(value =>
    isRecord(value.cache_creation) ? Number(value.cache_creation[field]) || 0 : 0));
  const cacheWrite5m = cacheWrite('ephemeral_5m_input_tokens');
  const cacheWrite1h = cacheWrite('ephemeral_1h_input_tokens');
  const flatCacheWrite = number('cache_creation_input_tokens');
  return {
    input_tokens: number('input_tokens'),
    output_tokens: number('output_tokens'),
    cache_read_input_tokens: number('cache_read_input_tokens'),
    cache_creation: {
      ephemeral_5m_input_tokens: cacheWrite5m + cacheWrite1h > 0 ? cacheWrite5m : flatCacheWrite,
      ephemeral_1h_input_tokens: cacheWrite1h,
    },
  };
}


interface BrokerProtocol {
  hostname: string;
  allowedPaths: Set<string>;
  upstreamPath(path: string): string;
  billable(path: string): boolean;
  headers(request: IncomingMessage): OutgoingHttpHeaders;
  parseRequest(body: Buffer, path: string): JsonRecord;
  inputTokenAdjustment?(payload: JsonRecord): number;
  outputLimit(payload: JsonRecord): number;
  responseUsage(body: Buffer, encoding?: string | string[]): JsonRecord | null;
}

function responsesUsage(body: Buffer, encoding?: string | string[], config?: BrokerConfig): JsonRecord | null {
  let response: JsonRecord | null = null;
  let failed = false;
  let metadata: JsonRecord | null = null;
  const accept = (value: unknown): void => {
    if (!isRecord(value)) return;
    if (isRecord(value.openrouter_metadata)) metadata = value.openrouter_metadata;
    if (value.type === 'error' || value.type === 'response.failed') failed = true;
    if ((value.type === 'response.completed' || value.type === 'response.incomplete'
      || (config?.provider === 'openrouter' && value.type === 'response.done')) && isRecord(value.response)) {
      response = value.response;
    } else if (value.object === 'response' && (value.status === 'completed' || value.status === 'incomplete')) {
      response = value;
    }
  };
  try {
    const text = decodedResponseBody(body, encoding).toString('utf8');
    try { accept(JSON.parse(text)); }
    catch {
      const parser = createParser({ maxBufferSize: MAX_REQUEST_BYTES,
        onError: () => { failed = true; },
        onEvent: ({ data }) => {
          if (data === '[DONE]') return;
          try { accept(JSON.parse(data)); } catch { failed = true; }
        },
      });
      parser.feed(`${text}\n\n`);
    }
  } catch { return null; }
  const usage = (response as JsonRecord | null)?.usage;
  if (failed || !isRecord(usage)) return null;
  const input = usage.input_tokens;
  const output = usage.output_tokens;
  const cached = isRecord(usage.input_tokens_details) ? usage.input_tokens_details.cached_tokens : 0;
  if (![input, output, cached].every(value => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0)
    || (cached as number) > (input as number)) return null;
  const normalized = { input_tokens: (input as number) - (cached as number), output_tokens: output,
    cache_read_input_tokens: cached, cache_creation_input_tokens: 0 };
  if (config?.provider !== 'openrouter') return normalized;
  const final = response as JsonRecord | null;
  const route = (final?.openrouter_metadata ?? metadata) as JsonRecord | null;
  const selected = isRecord(route?.endpoints) && Array.isArray(route.endpoints.available)
    ? route.endpoints.available.filter(endpoint => isRecord(endpoint) && endpoint.selected === true) : [];
  if (typeof usage.cost !== 'number' || !Number.isFinite(usage.cost) || usage.cost < 0
    || final?.model !== config.model || !route || route.requested !== config.model
    || route.strategy !== 'direct' || route.is_byok !== false || route.attempt !== 1
    || (route.pipeline !== undefined && (!Array.isArray(route.pipeline) || route.pipeline.length !== 0))
    || selected.length !== 1 || !isRecord(selected[0]) || selected[0].model !== config.model
    || typeof selected[0].provider !== 'string' || !selected[0].provider || selected[0].provider.length > 128) return null;
  return { ...normalized, provider_reported_cost_usd: usage.cost, upstream_provider: selected[0].provider };
}


function hasUnpricedInput(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(hasUnpricedInput);
  if (!isRecord(value)) return false;
  if (['input_file', 'item_reference'].includes(String(value.type))) return true;
  return Object.values(value).some(hasUnpricedInput);
}

// Only inline images have bounded input here. Provider receipts price actual tokens.
export function imageTokenAdjustment(value: unknown, model: string): number {
  if (Array.isArray(value)) return value.reduce((sum, item) => sum + imageTokenAdjustment(item, model), 0);
  if (!isRecord(value)) return 0;
  if (value.type === 'input_image') {
    if (!['gpt-6-astra', 'gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna', 'gpt-5.4', 'gpt-5.4-2026-03-05'].includes(model)
      || typeof value.image_url !== 'string'
      || !/^data:image\/(png|jpeg|webp|gif);base64,[A-Za-z0-9+/]+={0,2}$/.test(value.image_url)
      || value.file_id !== undefined) fail('image input requires inline data and a verified token bound');
    // OpenAI vision: 30,000 patches maximum x 1.2 tokens, plus rounding.
    // https://developers.openai.com/api/docs/guides/images-vision
    // Replace base64 text bytes rather than charging for both representations.
    return 36_001 - Buffer.byteLength(value.image_url, 'utf8');
  }
  return Object.values(value).reduce<number>((sum, item) => sum + imageTokenAdjustment(item, model), 0);
}

export function brokerProtocol(config: BrokerConfig): BrokerProtocol {
  if (!config.provider || config.provider === 'anthropic') return {
    hostname: 'api.anthropic.com',
    allowedPaths: new Set(['/v1/messages', '/v1/messages/count_tokens']),
    upstreamPath: path => path,
    billable: path => path === '/v1/messages',
    headers: request => upstreamHeaders(request, config),
    parseRequest: (body, path) => parseProviderRequest(body, path, config),
    outputLimit: payload => payload.max_tokens as number,
    responseUsage,
  };
  const router = config.provider === 'openrouter';
  if (router && (!/^[a-z0-9._-]+\/[a-zA-Z0-9._-]+$/.test(config.model)
    || config.model.startsWith('openrouter/') || /(?:^|[-/])latest$/.test(config.model))) {
    fail('OpenRouter requires one explicit model without routing variants');
  }
  const account = config.mode === 'subscription-token';
  if (account && !config.accountId) fail('OpenAI account identity is required');
  // Account Responses does not promise max_output_tokens. Use a documented model
  // bound, never assume an unknown model shares it. API requests enforce the cap.
  const accountOutputLimits: Record<string, number> = { 'gpt-5.3-codex': 128_000, 'gpt-5.4': 128_000, 'gpt-5.4-2026-03-05': 128_000,
    'gpt-5.6-sol': 128_000, 'gpt-6-astra': 128_000 };
  const outputLimit = account
    ? Object.hasOwn(accountOutputLimits, config.model) ? accountOutputLimits[config.model] : undefined
    : config.maxOutputTokens;
  if (!outputLimit) fail('OpenAI account model has no verified output-token bound');
  return {
    hostname: router ? 'openrouter.ai' : account ? 'chatgpt.com' : 'api.openai.com',
    allowedPaths: new Set(['/v1/responses']),
    upstreamPath: () => router ? '/api/v1/responses' : account ? '/backend-api/codex/responses' : '/v1/responses',
    billable: () => true,
    headers: request => {
      // OpenRouter routing/auth headers are trusted configuration, not agent input.
      const headers = router ? { 'content-type': 'application/json', accept: 'text/event-stream',
        'x-openrouter-metadata': 'enabled' } as OutgoingHttpHeaders : upstreamHeaders(request, config);
      delete headers['x-api-key'];
      for (const name of ['chatgpt-account-id', 'openai-organization', 'openai-project']) delete headers[name];
      headers.authorization = `Bearer ${config.credential}`;
      if (account) headers['chatgpt-account-id'] = config.accountId;
      return headers;
    },
    parseRequest: body => {
      const payload = JSON.parse(body.toString('utf8'));
      if (!isRecord(payload) || payload.model !== config.model) fail('request model does not match');
      imageTokenAdjustment(payload.input, config.model);
      // Token-only receipts cannot price hosted tools or hidden server-side input.
      if (payload.prompt || payload.previous_response_id || payload.conversation || hasUnpricedInput(payload.input)
        || payload.image_config !== undefined || payload.audio !== undefined
        || (payload.modalities !== undefined && (!Array.isArray(payload.modalities)
          || payload.modalities.some(modality => modality !== 'text')))
        || (payload.truncation !== undefined && payload.truncation !== 'disabled')
        || payload.background === true
        || (payload.service_tier !== undefined && payload.service_tier !== 'default' && payload.service_tier !== 'auto')
        || (payload.tools !== undefined && (!Array.isArray(payload.tools)
          || payload.tools.some(tool => !isRecord(tool) || !['function', 'custom'].includes(String(tool.type)))))) {
        fail('request requires unsupported pricing or server-side state');
      }
      if (router && ['provider', 'models', 'route', 'plugins', 'transforms', 'preset', 'user', 'session_id', 'trace', 'debug'].some(key => key in payload)) {
        fail('OpenRouter routing and transforms are owned by the broker');
      }
      const requested = payload.max_output_tokens;
      if (requested !== undefined && (!Number.isSafeInteger(requested) || (requested as number) < 1
        || (requested as number) > outputLimit)) fail('invalid max_output_tokens');
      if (!account) {
        payload.max_output_tokens = requested ?? outputLimit;
        payload.service_tier = 'default';
      }
      if (router) {
        const rates = config.pricingRates!;
        payload.provider = { only: [config.providerRoute], order: [config.providerRoute],
          allow_fallbacks: false, require_parameters: true,
          max_price: { prompt: rates.input, completion: rates.output, request: 0 } };
        payload.plugins = [];
        payload.transforms = [];
        payload.store = false;
      }
      return payload;
    },
    inputTokenAdjustment: payload => imageTokenAdjustment(payload.input, config.model),
    outputLimit: payload => account ? outputLimit : payload.max_output_tokens as number,
    responseUsage: (body, encoding) => responsesUsage(body, encoding, config),
  };
}
