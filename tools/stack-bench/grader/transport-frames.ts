import { brotliDecompressSync, gunzipSync } from 'node:zlib';

// A SpacetimeDB server frame carries a one-byte compression tag ahead of the
// message: 0 none, 1 brotli, 2 gzip, and the SDK compresses by default. The
// message text is inline UTF-8 once decoded, so a substring search finds it
// without the harness knowing the wire format. Any other frame is kept as it
// arrived.
export function transportFrameText(payload: string | Buffer): string {
  if (typeof payload === 'string') return payload;
  const bytes = Buffer.from(payload);
  if (bytes.length > 1) {
    try {
      if (bytes[0] === 1) return brotliDecompressSync(bytes.subarray(1)).toString('utf8');
      if (bytes[0] === 2) return gunzipSync(bytes.subarray(1)).toString('utf8');
    } catch { /* not a compressed SpacetimeDB frame */ }
  }
  return bytes.toString('utf8');
}
