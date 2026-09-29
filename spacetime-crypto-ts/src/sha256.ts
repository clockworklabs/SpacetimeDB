import { sha256 as nobleSha256 } from '@noble/hashes/sha2.js';

export function sha256(data: Uint8Array): Uint8Array {
  return nobleSha256(data);
}

export const SHA256_BYTES = 32;
