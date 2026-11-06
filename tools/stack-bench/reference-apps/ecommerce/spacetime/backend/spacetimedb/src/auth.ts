import { t, type InferSchema, type ReducerCtx, type ViewCtx } from 'spacetimedb/server';
import { scrypt } from '@noble/hashes/scrypt.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import spacetimedb from './schema';

type S = InferSchema<typeof spacetimedb>;
type Ctx = ReducerCtx<S>;
const SESSION_MICROS = 24n * 60n * 60n * 1_000_000n;

export function hashPassword(password: string, salt: string): string {
  return bytesToHex(scrypt(password, salt, {
    N: 131072, r: 8, p: 1, dkLen: 32, maxmem: 256 * 1024 * 1024,
  }));
}

function sameHash(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let difference = 0;
  for (let i = 0; i < a.length; i++) difference |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return difference === 0;
}

function validInput(name: string, password: string): boolean {
  return /^[A-Za-z0-9-]{1,48}$/.test(name) && password.length > 0 && password.length <= 64;
}

export function getAccountId(ctx: Ctx | ViewCtx<S>): bigint | null {
  const binding = ctx.db.session.identity.find(ctx.sender);
  // Views have no clock. The existing maintenance tick removes expired bindings.
  if (!binding || ('timestamp' in ctx && binding.expiresMicros <= ctx.timestamp.microsSinceUnixEpoch)) return null;
  return binding.accountId;
}

function bindSession(ctx: Ctx, accountId: bigint) {
  ctx.db.session.identity.delete(ctx.sender);
  ctx.db.session.insert({ identity: ctx.sender, accountId,
    expiresMicros: ctx.timestamp.microsSinceUnixEpoch + SESSION_MICROS });
}

// Passwords enter procedures, not reducers: reducer arguments enter the commit log.
export const signUp = spacetimedb.procedure(
  { name: t.string(), password: t.string(), salt: t.string() }, t.bool(),
  (ctx, { name, password, salt }) => {
    ctx.withTx(tx => { tx.db.session.identity.delete(ctx.sender); });
    if (!validInput(name, password) || !/^[a-f0-9]{64}$/.test(salt)) return false;
    // Web Crypto supplies the public salt. Include the server-verified native identity
    // so a caller cannot choose another account's salt. No timestamp RNG is used.
    const passwordSalt = `${ctx.sender.toHexString()}:${salt}`;
    const passwordHash = hashPassword(password, passwordSalt);
    return ctx.withTx(tx => {
      if (tx.db.account.username.find(name) || tx.db.account.passwordSalt.find(passwordSalt)) return false;
      const account = tx.db.account.insert({ id: 0n, username: name, passwordSalt, passwordHash,
        isAdmin: false, isStaff: false });
      bindSession(tx, account.id);
      return true;
    });
  }
);

export const signIn = spacetimedb.procedure(
  { name: t.string(), password: t.string() }, t.bool(),
  (ctx, { name, password }) => {
    const account = ctx.withTx(tx => {
      tx.db.session.identity.delete(ctx.sender);
      return tx.db.account.username.find(name);
    });
    if (!validInput(name, password)) return false;
    // Keep the expensive verification path for nonexistent accounts too.
    const digest = hashPassword(password, account?.passwordSalt ?? 'unknown-account');
    const valid = !!account && sameHash(digest, account.passwordHash);
    if (!valid) return false;
    return ctx.withTx(tx => {
      const current = tx.db.account.id.find(account.id);
      if (!current || current.passwordHash !== account.passwordHash) return false;
      bindSession(tx, account.id);
      return true;
    });
  }
);

export const signOut = spacetimedb.reducer(ctx => { ctx.db.session.identity.delete(ctx.sender); });
