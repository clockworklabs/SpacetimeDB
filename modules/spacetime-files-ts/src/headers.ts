import { FILE_VISIBILITY_PUBLIC } from './constants.js';

// Rendered in the browser. Everything else downloads, so stored HTML or SVG
// never runs on the serving origin.
const INLINE_MIME_TYPES = new Set([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'image/avif',
]);

export function responseHeaders(file: {
  mimeType: string;
  size: bigint;
  sha256Hex: string;
  visibility: string;
}): Record<string, string> {
  const headers: Record<string, string> = {
    'content-type': file.mimeType,
    'content-length': String(file.size),
    etag: `"${file.sha256Hex}"`,
    'cache-control':
      file.visibility === FILE_VISIBILITY_PUBLIC
        ? 'public, max-age=300, must-revalidate'
        : 'private, max-age=60, must-revalidate',
    'x-content-type-options': 'nosniff',
    'content-security-policy': 'sandbox',
  };
  if (!INLINE_MIME_TYPES.has(file.mimeType))
    headers['content-disposition'] = 'attachment';
  return headers;
}
