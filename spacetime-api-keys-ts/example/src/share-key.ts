export function shareKeyFromHash(hash: string): string | null {
  if (!hash.startsWith('#')) return null;
  const key = new URLSearchParams(hash.slice(1)).get('key')?.trim();
  return key || null;
}

export function parseShareKey(raw: string): string | null {
  const value = raw.trim();
  if (!value) return null;

  try {
    return shareKeyFromHash(new URL(value).hash);
  } catch {
    return value;
  }
}
