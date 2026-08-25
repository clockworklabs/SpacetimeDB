export interface HttpLike {
  fetch(
    url: string,
    init: { method: string; headers: Record<string, string>; body?: string }
  ): {
    status: number;
    text(): string;
  };
}

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'image'; mimeType: string; data: string };

export type ChatMessage =
  | {
      role: 'system' | 'user' | 'assistant';
      content: string | ContentBlock[];
      tool_calls?: ToolCall[];
    }
  | { role: 'tool'; tool_call_id: string; content: string };

export type ToolCall = {
  id: string;
  type: 'function';
  function: { name: string; arguments: string };
};

export type ToolDefinition = {
  type: 'function';
  function: {
    name: string;
    description: string;
    parameters: {
      type: 'object';
      properties: Record<string, unknown>;
      required?: string[];
    };
  };
};

export type ResponseFormat = { type: string; [k: string]: unknown };

export type ChatRequest = {
  apiKey: string;
  model: string;
  system?: string;
  messages: ChatMessage[];
  tools?: ToolDefinition[];
  maxTokens?: number;
  responseFormat?: ResponseFormat;
  /** Back-to-back retries on 429/5xx/transport (STDB has no sleep). */
  retries?: number;
};

export type ChatResponse = {
  text: string | null;
  toolCalls: ToolCall[];
  finishReason: 'stop' | 'tool_calls' | 'length' | 'content_filter' | string;
  usage: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
  model: string;
  raw: unknown;
};

export type ChatError =
  | { kind: 'http'; status: number; body: string }
  | { kind: 'transport'; message: string }
  | { kind: 'parse'; message: string; body: string };

export type ChatResult =
  | { ok: true; response: ChatResponse }
  | { ok: false; error: ChatError };

export type ParsedResponse = Omit<ChatResponse, never>;

export interface Provider {
  name: string;
  buildRequest(req: ChatRequest): {
    url: string;
    headers: Record<string, string>;
    body: string;
  };
  parseResponse(text: string, requestedModel: string): ParsedResponse;
}

export function isRetryableError(err: ChatError): boolean {
  if (err.kind === 'transport') return true;
  if (err.kind === 'http') {
    return (
      err.status === 429 ||
      err.status === 502 ||
      err.status === 503 ||
      err.status === 504
    );
  }
  return false;
}

export function callChat(
  http: HttpLike,
  provider: Provider,
  req: ChatRequest
): ChatResult {
  const retries = Math.max(0, req.retries ?? 0);
  let lastError: ChatError | null = null;
  for (let attempt = 0; attempt <= retries; attempt++) {
    const result = callChatOnce(http, provider, req);
    if (result.ok) return result;
    lastError = result.error;
    if (!isRetryableError(result.error)) return result;
  }
  return {
    ok: false,
    error: lastError ?? { kind: 'transport', message: 'no attempts made' },
  };
}

function callChatOnce(
  http: HttpLike,
  provider: Provider,
  req: ChatRequest
): ChatResult {
  const { url, headers, body } = provider.buildRequest(req);

  let res: { status: number; text(): string };
  try {
    res = http.fetch(url, { method: 'POST', headers, body });
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'transport',
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }

  const text = res.text();
  if (res.status < 200 || res.status >= 300) {
    return {
      ok: false,
      error: { kind: 'http', status: res.status, body: text },
    };
  }

  try {
    return { ok: true, response: provider.parseResponse(text, req.model) };
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'parse',
        message: err instanceof Error ? err.message : String(err),
        body: text,
      },
    };
  }
}
