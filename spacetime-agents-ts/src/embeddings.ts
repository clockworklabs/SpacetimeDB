import type { HttpLike } from './openrouter.ts';

export interface EmbeddingProvider {
  name: string;
  embed(
    http: HttpLike,
    apiKey: string,
    model: string,
    texts: string[]
  ): EmbeddingResult;
}

export type EmbeddingResult =
  | {
      ok: true;
      vectors: number[][];
      model: string;
      usage: { promptTokens: number; totalTokens: number };
    }
  | {
      ok: false;
      error:
        | { kind: 'http'; status: number; body: string }
        | { kind: 'transport'; message: string }
        | { kind: 'parse'; message: string; body: string };
    };

function asObject(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function tokenCount(value: unknown): number {
  const count = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(count) && count >= 0
    ? Math.min(Math.trunc(count), Number.MAX_SAFE_INTEGER)
    : 0;
}

function postOpenAiEmbeddings(
  http: HttpLike,
  url: string,
  apiKey: string,
  model: string,
  texts: string[]
): EmbeddingResult {
  if (
    texts.length === 0 ||
    texts.length > 100 ||
    texts.some(text => text.length > 32_768)
  ) {
    return {
      ok: false,
      error: { kind: 'parse', message: 'invalid embedding input', body: '' },
    };
  }
  if (texts.reduce((total, value) => total + value.length, 0) > 262_144) {
    return {
      ok: false,
      error: {
        kind: 'parse',
        message: 'embedding input is too large',
        body: '',
      },
    };
  }
  let res: { status: number; text(): string };
  try {
    res = http.fetch(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ model, input: texts }),
    });
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
      error: { kind: 'http', status: res.status, body: text.slice(0, 65_536) },
    };
  }
  try {
    if (text.length > 4 * 1024 * 1024)
      throw new Error('embedding response is too large');
    const parsed: unknown = JSON.parse(text);
    const root = asObject(parsed);
    if (!root) throw new Error('embedding response must be an object');
    const data = root?.data;
    if (!Array.isArray(data)) throw new Error('no data array');
    const vectors: number[][] = data.map(value => {
      const row = asObject(value);
      if (!Array.isArray(row?.embedding)) throw new Error('missing embedding');
      if (row.embedding.length === 0 || row.embedding.length > 16_384) {
        throw new Error('invalid embedding dimensions');
      }
      if (
        !row.embedding.every(
          component =>
            typeof component === 'number' && Number.isFinite(component)
        )
      ) {
        throw new Error('embedding contains a non-finite value');
      }
      return row.embedding as number[];
    });
    const usage = asObject(root.usage);
    return {
      ok: true,
      vectors,
      model: String(root.model ?? model),
      usage: {
        promptTokens: tokenCount(usage?.prompt_tokens),
        totalTokens: tokenCount(usage?.total_tokens),
      },
    };
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'parse',
        message: err instanceof Error ? err.message : String(err),
        body: text.slice(0, 65_536),
      },
    };
  }
}

export const openAiEmbeddingsProvider: EmbeddingProvider = {
  name: 'openai',
  embed: (http, apiKey, model, texts) =>
    postOpenAiEmbeddings(
      http,
      'https://api.openai.com/v1/embeddings',
      apiKey,
      model,
      texts
    ),
};

export const openRouterEmbeddingsProvider: EmbeddingProvider = {
  name: 'openrouter',
  embed: (http, apiKey, model, texts) =>
    postOpenAiEmbeddings(
      http,
      'https://openrouter.ai/api/v1/embeddings',
      apiKey,
      model,
      texts
    ),
};

export const BUILT_IN_EMBEDDING_PROVIDERS: Record<string, EmbeddingProvider> = {
  openai: openAiEmbeddingsProvider,
  openrouter: openRouterEmbeddingsProvider,
};

export function cosineSimilarity(
  a: ArrayLike<number>,
  b: ArrayLike<number>
): number {
  if (a.length !== b.length || a.length === 0) return 0;
  let dot = 0,
    na = 0,
    nb = 0;
  for (let i = 0; i < a.length; i++) {
    const x = a[i],
      y = b[i];
    dot += x * y;
    na += x * x;
    nb += y * y;
  }
  const denom = Math.sqrt(na) * Math.sqrt(nb);
  return denom === 0 ? 0 : dot / denom;
}

export function topKByScore<T>(
  items: T[],
  scoreFn: (t: T) => number,
  k: number
): { item: T; score: number }[] {
  const scored = items.map(item => ({ item, score: scoreFn(item) }));
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, Math.max(0, k));
}
