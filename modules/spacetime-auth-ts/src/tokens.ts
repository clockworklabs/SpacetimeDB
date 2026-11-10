import type { AuthTransactionCtx } from './context.js';
import { base64UrlEncode, deriveSecret, uuidV7 } from './crypto.js';
import { requireConfig } from './handlers/http.js';
import { privateKeyFromPem } from './keys.js';

/** 32 bytes that are unpredictable without the signing key. */
export function secretBytes(tx: AuthTransactionCtx): Uint8Array {
  const cfg = requireConfig(tx);
  const tokenCounter = cfg.tokenCounter + 1n;
  tx.db.authConfig.singleton.update({ ...cfg, tokenCounter });
  return deriveSecret(
    privateKeyFromPem(cfg.es256PrivateKeyPem),
    tokenCounter,
    tx.timestamp.microsSinceUnixEpoch
  );
}

export function newToken(tx: AuthTransactionCtx): string {
  return base64UrlEncode(secretBytes(tx));
}

export function newId(tx: AuthTransactionCtx): string {
  return uuidV7(tx.timestamp.microsSinceUnixEpoch / 1000n, secretBytes(tx));
}
