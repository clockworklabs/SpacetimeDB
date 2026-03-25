import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';

function formatErrorWithCause(err: unknown): string {
  if (!(err instanceof Error)) {
    return String(err);
  }

  const cause =
    'cause' in err && err.cause != null ? `; cause: ${String(err.cause)}` : '';
  return `${err.message}${cause}`;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function fetchJson<T>(
  url: string,
  init?: RequestInit,
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(url, init);
  } catch (err) {
    throw new Error(
      `${init?.method ?? 'GET'} ${url} failed: ${formatErrorWithCause(err)}`,
    );
  }

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${init?.method ?? 'GET'} ${url} failed: ${res.status} ${text}`);
  }
  return (await res.json()) as T;
}

export async function postJson<TResponse>(
  baseUrl: string,
  path: string,
  body: unknown,
): Promise<TResponse> {
  return await fetchJson<TResponse>(new URL(path, baseUrl).toString(), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export async function getJson<TResponse>(
  baseUrl: string,
  path: string,
): Promise<TResponse> {
  return await fetchJson<TResponse>(new URL(path, baseUrl).toString());
}

export async function retryUntilSuccess<T>(
  label: string,
  op: () => Promise<T>,
  retryDelayMs = 1000,
  maxAttempts = 3,
  shouldRetry: () => boolean = () => true,
): Promise<T> {
  let attempts = 0;

  for (;;) {
    try {
      attempts += 1;
      return await op();
    } catch (err) {
      if (!shouldRetry() || attempts >= maxAttempts) {
        throw err;
      }

      const msg = err instanceof Error ? err.message : String(err);
      console.warn(`${label}: ${msg}`);
      await sleep(retryDelayMs);
    }
  }
}

export async function writeJsonFile(path: string, value: unknown): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

export function isoNow(ms = Date.now()): string {
  return new Date(ms).toISOString();
}
