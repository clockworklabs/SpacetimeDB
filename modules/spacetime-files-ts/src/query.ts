export function queryParam(uri: string, name: string): string | undefined {
  const queryIdx = uri.indexOf('?');
  if (queryIdx < 0) return undefined;
  for (const part of uri.slice(queryIdx + 1).split('&')) {
    if (!part) continue;
    const eqIdx = part.indexOf('=');
    const rawKey = eqIdx < 0 ? part : part.slice(0, eqIdx);
    try {
      if (decodeURIComponent(rawKey.replace(/\+/g, ' ')) !== name) continue;
      const rawValue = eqIdx < 0 ? '' : part.slice(eqIdx + 1);
      return decodeURIComponent(rawValue.replace(/\+/g, ' '));
    } catch {
      return undefined;
    }
  }
  return undefined;
}
