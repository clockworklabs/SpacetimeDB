export type Visibility = 'owner' | 'public';

export function normalizePath(
  path: string,
  kind: 'file' | 'folder' = 'folder'
): string {
  let normalized = String(path || '')
    .trim()
    .replaceAll('\\', '/')
    .replace(/\/+/g, '/');
  if (!normalized.startsWith('/')) normalized = '/' + normalized;
  if (normalized.length > 1 && normalized.endsWith('/')) {
    normalized = normalized.slice(0, -1);
  }
  if (kind === 'file' && normalized === '/') {
    throw new Error('file path required');
  }
  return normalized;
}

export function parentPath(path: string): string {
  if (path === '/') return '/';
  const separatorIndex = path.lastIndexOf('/');
  return separatorIndex <= 0 ? '/' : path.slice(0, separatorIndex);
}

export function baseName(path: string): string {
  if (path === '/') return '/';
  return path.slice(path.lastIndexOf('/') + 1);
}

export function joinPath(directory: string, name: string): string {
  return directory === '/' ? `/${name}` : `${directory}/${name}`;
}

// '/docs' must not match '/docs2'.
export function childPrefix(path: string): string {
  return path === '/' ? '/' : path + '/';
}

export function fileUrl(id: bigint): string {
  return `/files?id=${encodeURIComponent(String(id))}`;
}
