function parseStdbUrl(rawUrl: string): URL {
  const trimmed = rawUrl.trim();
  if (!trimmed) {
    throw new Error('STDB_URL not set');
  }

  const withScheme = /^[a-z]+:\/\//i.test(trimmed)
    ? trimmed
    : `ws://${trimmed}`;
  return new URL(withScheme);
}

export function normalizeStdbUrl(rawUrl: string): string {
  return parseStdbUrl(rawUrl).host;
}

export function deriveWebsocketUrl(rawUrl: string): string {
  const url = parseStdbUrl(rawUrl);
  url.protocol = 'ws:';
  url.pathname = '';
  url.search = '';
  url.hash = '';
  return url.toString();
}
