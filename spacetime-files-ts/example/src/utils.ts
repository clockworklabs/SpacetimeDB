import type { Timestamp } from 'spacetimedb';

export type Visibility = 'owner' | 'public';

export interface ServerConfig {
  stdbUri: string;
  appDatabase: string;
}

// Connection + token persistence

// Persisted token = same identity (and files) across reloads.
export const TOKEN_KEY = 'vault:auth-token';

export function loadToken(): string | undefined {
  try {
    return localStorage.getItem(TOKEN_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

export function saveToken(token: string | undefined): void {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
  } catch {
    /* storage unavailable; token stays in-memory only */
  }
}

export function clearToken(): void {
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    /* ignore */
  }
}

// Path + formatting helpers

export function normalizePath(
  path: string,
  kind: 'file' | 'folder' = 'folder'
): string {
  let out = String(path || '')
    .trim()
    .replaceAll('\\', '/')
    .replace(/\/+/g, '/');
  if (!out.startsWith('/')) out = '/' + out;
  if (out.length > 1 && out.endsWith('/')) out = out.slice(0, -1);
  if (kind === 'file' && out === '/') throw new Error('file path required');
  return out;
}
export function parentPath(path: string): string {
  if (path === '/') return '/';
  const idx = path.lastIndexOf('/');
  return idx <= 0 ? '/' : path.slice(0, idx);
}
export function baseName(path: string): string {
  if (path === '/') return '/';
  return path.slice(path.lastIndexOf('/') + 1);
}
export function joinPath(dir: string, name: string): string {
  return dir === '/' ? `/${name}` : `${dir}/${name}`;
}
// '/docs' must not match '/docs2'.
export function childPrefix(path: string): string {
  return path === '/' ? '/' : path + '/';
}
export function fileUrl(id: bigint): string {
  return `/files?id=${encodeURIComponent(String(id))}`;
}
export function fmtSize(value: number | bigint | string | undefined): string {
  const n = Number(value ?? 0);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
export function tsMs(ts: Timestamp | undefined): number {
  if (!ts) return 0;
  try {
    return Number(ts.microsSinceUnixEpoch / 1000n);
  } catch {
    return 0;
  }
}
export function fmtWhen(ts: Timestamp | undefined): string {
  const ms = tsMs(ts);
  if (!ms) return '';
  const d = new Date(ms);
  if (d.toDateString() === new Date().toDateString()) {
    return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  }
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}
export function escapeHtml(s: unknown): string {
  return String(s ?? '').replace(
    /[&<>"']/g,
    c =>
      ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[
        c
      ]!
  );
}
export function kindClass(mime: string | undefined): {
  cls: string;
  ico: string;
} {
  if (!mime) return { cls: 'generic', ico: 'file' };
  if (mime.startsWith('image/')) return { cls: 'image', ico: 'file-image' };
  if (mime.startsWith('audio/') || mime.startsWith('video/'))
    return { cls: 'media', ico: 'file-media' };
  if (mime.startsWith('text/') || mime === 'application/json')
    return { cls: 'text', ico: 'file-text' };
  return { cls: 'generic', ico: 'file' };
}

// Error mapping: turn server codes into human sentences

// Errors are `<code>:<detail>`. Parse only the code because detail can contain user paths.
export const ERROR_MESSAGES: Record<string, string> = {
  'vault.folder_not_empty':
    "That folder isn't empty. Delete its contents first.",
  'vault.folder_exists': 'A folder with that name already exists here.',
  'vault.file_exists':
    'A file with that name already exists at the destination.',
  'vault.parent_not_found': "That destination folder doesn't exist.",
  'vault.folder_not_found': "That folder doesn't exist.",
  'vault.file_not_found': "That file doesn't exist.",
  'vault.cannot_delete_root': "The root folder can't be deleted.",
  'vault.cannot_rename_root': "The root folder can't be renamed.",
  'vault.invalid_file_path': 'A file needs a name.',
  'vault.invalid_path': "That name isn't allowed.",
  'vault.invalid_visibility': 'That visibility value is invalid.',
  'files.invalid_path': "That name isn't allowed.",
  'files.invalid_visibility': 'That visibility value is invalid.',
  'files.not_found': "That file doesn't exist.",
  'files.invalid_mime_type': 'That file type is invalid.',
};
export function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err ?? '');
}
export function errorCode(err: unknown): string {
  return (errorMessage(err).match(/\b(?:vault|files)\.[a-z_]+/) ?? [])[0] ?? '';
}
export function humanError(err: unknown, ctx: { name?: string } = {}): string {
  const raw = errorMessage(err) || 'Something went wrong';
  const big = raw.match(/^files\.too_large:(\d+)\/(\d+)/);
  if (big) {
    const name = ctx.name ? `"${ctx.name}"` : 'That file';
    return `${name} is ${fmtSize(big[1])}. Vault caps files at ${fmtSize(big[2])}.`;
  }
  return ERROR_MESSAGES[errorCode(err)] ?? raw;
}
