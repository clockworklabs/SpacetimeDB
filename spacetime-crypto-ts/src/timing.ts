/**
 * Compare two byte arrays in time independent of their content. Returns true
 * iff both have the same length AND identical bytes.
 *
 * IMPORTANT: this is only constant-time when arrays are equal length. The
 * length check is up front and leaks length info, which is fine for fixed-size
 * tags (HMAC outputs, JWT signatures). For variable-length comparisons you'd
 * need to also pad to a max length.
 *
 * Note: the JIT can sometimes shortcut bitwise OR chains. The accumulator
 * pattern below is the standard timing-safe construction; it's about as
 * practical in pure JS without going to WASM.
 */
export function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff === 0;
}

/** Decode `01ab23cd` into Uint8Array. Throws on odd length or non-hex. */
export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error('hex: odd length');
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    const hi = hexNibble(hex.charCodeAt(i * 2));
    const lo = hexNibble(hex.charCodeAt(i * 2 + 1));
    out[i] = (hi << 4) | lo;
  }
  return out;
}

function hexNibble(code: number): number {
  if (code >= 0x30 && code <= 0x39) return code - 0x30;
  if (code >= 0x61 && code <= 0x66) return code - 0x61 + 10;
  if (code >= 0x41 && code <= 0x46) return code - 0x41 + 10;
  throw new Error(`hex: non-hex char ${String.fromCharCode(code)}`);
}

/** Encode bytes as lowercase hex. */
export function bytesToHex(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i++) {
    s += bytes[i].toString(16).padStart(2, '0');
  }
  return s;
}

/** Decode standard base64 (with or without padding). Throws on invalid input. */
export function base64ToBytes(b64: string): Uint8Array {
  if (/\s/.test(b64)) throw new Error('base64: whitespace is not allowed');
  if (b64.length % 4 === 1) throw new Error('base64: invalid length');
  const firstPad = b64.indexOf('=');
  if (firstPad >= 0) {
    const padding = b64.length - firstPad;
    if (
      padding > 2 ||
      b64.length % 4 !== 0 ||
      !/^=+$/.test(b64.slice(firstPad))
    ) {
      throw new Error('base64: invalid padding');
    }
  }

  let s = b64;
  while (s.length % 4 !== 0) s += '=';
  const ALPHABET =
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  const lookup = new Int8Array(256).fill(-1);
  for (let i = 0; i < ALPHABET.length; i++) lookup[ALPHABET.charCodeAt(i)] = i;
  lookup[0x3d /* '=' */] = 0; // pad treated as 0; we trim afterwards

  // Count pad to compute output length.
  let pad = 0;
  if (s.endsWith('==')) pad = 2;
  else if (s.endsWith('=')) pad = 1;

  const out = new Uint8Array((s.length / 4) * 3 - pad);
  let oi = 0;
  for (let i = 0; i < s.length; i += 4) {
    const c0 = lookup[s.charCodeAt(i)];
    const c1 = lookup[s.charCodeAt(i + 1)];
    const c2 = lookup[s.charCodeAt(i + 2)];
    const c3 = lookup[s.charCodeAt(i + 3)];
    if (c0 < 0 || c1 < 0 || c2 < 0 || c3 < 0) {
      throw new Error('base64: invalid char');
    }
    const finalQuartet = i + 4 === s.length;
    if (
      s[i] === '=' ||
      s[i + 1] === '=' ||
      (!finalQuartet && (s[i + 2] === '=' || s[i + 3] === '='))
    ) {
      throw new Error('base64: invalid padding');
    }
    if (s[i + 2] === '=' && s[i + 3] !== '=')
      throw new Error('base64: invalid padding');
    if (s[i + 2] === '=' && (c1 & 0x0f) !== 0)
      throw new Error('base64: non-canonical padding');
    if (s[i + 3] === '=' && s[i + 2] !== '=' && (c2 & 0x03) !== 0) {
      throw new Error('base64: non-canonical padding');
    }
    const v = (c0 << 18) | (c1 << 12) | (c2 << 6) | c3;
    if (oi < out.length) out[oi++] = (v >> 16) & 0xff;
    if (oi < out.length) out[oi++] = (v >> 8) & 0xff;
    if (oi < out.length) out[oi++] = v & 0xff;
  }
  return out;
}
