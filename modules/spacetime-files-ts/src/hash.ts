import { sha256 } from '@spacetimedb/crypto';

export function fileSha256Hex(bytes: Uint8Array | number[]): string {
  const digest = sha256(
    bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes)
  );
  let out = '';
  for (const byte of digest) out += byte.toString(16).padStart(2, '0');
  return out;
}
