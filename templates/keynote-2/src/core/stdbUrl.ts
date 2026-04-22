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

export function deriveMetricsUrl(rawUrl: string): string {
  const url = parseStdbUrl(rawUrl);
  url.protocol = 'http:';
  url.pathname = '/v1/metrics';
  url.search = '';
  url.hash = '';
  return url.toString();
}

export function deriveWebsocketUrl(rawUrl: string): string {
  const url = parseStdbUrl(rawUrl);
  url.protocol = 'ws:';
  url.pathname = '';
  url.search = '';
  url.hash = '';
  return url.toString();
}
