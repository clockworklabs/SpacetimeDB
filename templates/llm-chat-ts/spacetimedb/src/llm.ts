export interface HttpLike {
  fetch(url: string, init: { method: string; headers: Record<string, string>; body?: string }): {
    status: number;
    text(): string;
  };
}

export type ChatMessage = {
  role: 'system' | 'user' | 'assistant';
  content: string;
};

export type ChatRequest = {
  apiKey: string;
  model: string;
  messages: ChatMessage[];
};

export type ChatResponse = {
  text: string;
  model: string;
};

export type ChatError =
  | { kind: 'http'; status: number; body: string }
  | { kind: 'transport'; message: string }
  | { kind: 'parse'; message: string; body: string };

export type ChatResult =
  | { ok: true; response: ChatResponse }
  | { ok: false; error: ChatError };

export interface Provider {
  name: string;
  buildRequest(req: ChatRequest): {
    url: string;
    headers: Record<string, string>;
    body: string;
  };
  parseResponse(text: string, requestedModel: string): ChatResponse;
}

function buildOpenAiBody(req: ChatRequest): string {
  return JSON.stringify({
    model: req.model,
    messages: req.messages,
  });
}

function parseOpenAiResponse(text: string, requestedModel: string): ChatResponse {
  const parsed = JSON.parse(text);
  const content = parsed?.choices?.[0]?.message?.content;
  if (typeof content !== 'string' || content.length === 0) {
    throw new Error('response did not include choices[0].message.content');
  }
  return {
    text: content,
    model: String(parsed.model ?? requestedModel),
  };
}

export const openRouterProvider: Provider = {
  name: 'openrouter',
  buildRequest(req) {
    return {
      url: 'https://openrouter.ai/api/v1/chat/completions',
      headers: {
        Authorization: `Bearer ${req.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: buildOpenAiBody(req),
    };
  },
  parseResponse: parseOpenAiResponse,
};

export const openAiProvider: Provider = {
  name: 'openai',
  buildRequest(req) {
    return {
      url: 'https://api.openai.com/v1/chat/completions',
      headers: {
        Authorization: `Bearer ${req.apiKey}`,
        'Content-Type': 'application/json',
      },
      body: buildOpenAiBody(req),
    };
  },
  parseResponse: parseOpenAiResponse,
};

export const providers: Record<string, Provider> = {
  openrouter: openRouterProvider,
  openai: openAiProvider,
};

export function callChat(http: HttpLike, provider: Provider, req: ChatRequest): ChatResult {
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

  const responseText = res.text();
  if (res.status < 200 || res.status >= 300) {
    return { ok: false, error: { kind: 'http', status: res.status, body: responseText } };
  }

  try {
    return { ok: true, response: provider.parseResponse(responseText, req.model) };
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'parse',
        message: err instanceof Error ? err.message : String(err),
        body: responseText,
      },
    };
  }
}

export function formatChatError(err: ChatError): string {
  switch (err.kind) {
    case 'http':
      return `LLM HTTP ${err.status}: ${truncate(err.body, 500)}`;
    case 'transport':
      return `LLM transport error: ${err.message}`;
    case 'parse':
      return `LLM parse error: ${err.message}`;
  }
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : text.slice(0, max) + '...';
}

