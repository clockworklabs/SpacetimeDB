import { Timestamp } from 'spacetimedb';
import {
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import { base64ToBytes } from '@spacetimedb/crypto';
import submodule from './submodule.js';
import {
  credentialExpiry,
  resolveConfig,
  S3Error,
  type Credentials,
  type S3Config,
} from './config.js';
import { storage, PAGE_SIZE, type Http } from './storage.js';
import type { FileRecord } from './model.js';

type S3Tx = ReducerCtx<InferSchema<typeof submodule>>;
type HostTx = Pick<S3Tx, 'timestamp'>;
type Procedure<Tx> = { http: Http; withTx<T>(body: (tx: Tx) => T): T };
export interface UploadInput {
  owner: string;
  requestKey: string;
  filename: string;
  contentType: string;
  size: number;
  checksum: string;
}
export interface FileInput {
  owner: string;
  id: string;
}
const later = (now: Timestamp, seconds: number) =>
  new Timestamp(now.microsSinceUnixEpoch + BigInt(seconds) * 1_000_000n);
const millis = (now: Timestamp) => Number(now.microsSinceUnixEpoch / 1000n);
const due = (now: Timestamp) =>
  new Range<Timestamp>(null, { tag: 'included', value: now });
// File IDs are u64 values, passed through the API as decimal strings.
const parseId = (id: string) =>
  /^[1-9]\d{0,19}$/.test(id) && BigInt(id) < 2n ** 64n ? BigInt(id) : undefined;

/** The host authorizes callers and supplies the owner key. */
export function client<Tx extends HostTx>(
  input: S3Config & { submodule(tx: Tx): S3Tx }
) {
  const config = resolveConfig(input);
  const target = JSON.stringify([
    config.bucket,
    config.region,
    config.prefix,
    config.accountId,
  ]);
  const tables = (tx: Tx) => input.submodule(tx).db;
  function db(tx: Tx) {
    const value = tables(tx);
    if (value.cleanup.id.find(0)?.target !== target)
      throw new S3Error('s3.target_changed_or_not_initialized');
    return value;
  }
  function owned(tx: Tx, { owner, id }: FileInput) {
    const table = db(tx).file,
      key = parseId(id),
      file = key === undefined ? undefined : table.id.find(key);
    if (!file || !owner || file.owner !== owner)
      throw new SenderError('s3.file_not_found');
    return file;
  }
  const installation = (tx: Tx) =>
    db(tx).cleanup.id.find(0)!.installedAt.microsSinceUnixEpoch;
  function service(ctx: Procedure<Tx>, credentials: Credentials) {
    return storage(
      config,
      ctx.withTx(installation),
      credentials,
      ctx.http,
      () => ctx.withTx(tx => tx.timestamp)
    );
  }
  type Service = ReturnType<typeof service>;
  function processFile(ctx: Procedure<Tx>, provider: Service, id: bigint) {
    const claimed = ctx.withTx(tx => {
      const table = db(tx).file,
        file = table.id.find(id);
      if (
        !file ||
        file.nextAttemptAt.microsSinceUnixEpoch >
          tx.timestamp.microsSinceUnixEpoch
      )
        return;
      if (file.state.tag === 'Deleted') {
        table.id.delete(id);
        return;
      }
      if (file.state.tag === 'Ready') return;
      const expired =
        file.state.tag === 'Pending' ||
        (file.state.tag === 'Copying' &&
          tx.timestamp.microsSinceUnixEpoch >=
            later(file.uploadExpiresAt, config.recoverySeconds)
              .microsSinceUnixEpoch);
      const next: FileRecord = {
        ...file,
        state: expired ? { tag: 'Deleting' } : file.state,
        nextAttemptAt: later(tx.timestamp, 6 * config.httpTimeoutSeconds + 10),
        attempt: file.attempt + 1n,
      };
      table.id.update(next);
      return next;
    });
    if (!claimed) return;
    let error: string | undefined;
    try {
      if (claimed.state.tag === 'Copying') provider.copyVerified(claimed);
      else provider.removeFile(id);
    } catch (cause) {
      if (!(cause instanceof S3Error)) throw cause;
      error = cause.message;
    }
    ctx.withTx(tx => {
      const tables = db(tx),
        file = tables.file.id.find(id);
      // A late HTTP reply cannot overwrite deletion or a newer attempt.
      if (
        !file ||
        file.state.tag !== claimed.state.tag ||
        file.attempt !== claimed.attempt
      )
        return;
      if (error) {
        tables.file.id.update({
          ...file,
          lastError: error,
          nextAttemptAt: later(
            tx.timestamp,
            Math.min(300, 2 ** Number(file.attempt > 8n ? 8n : file.attempt))
          ),
        });
        return;
      }
      if (file.state.tag === 'Deleting') {
        const usage = tables.ownerUsage.owner.find(file.owner);
        if (!usage || usage.count < 1 || usage.bytes < BigInt(file.size))
          throw new Error('s3.quota_invariant');
        if (usage.count === 1) tables.ownerUsage.owner.delete(file.owner);
        else
          tables.ownerUsage.owner.update({
            ...usage,
            count: usage.count - 1,
            bytes: usage.bytes - BigInt(file.size),
          });
      }
      tables.file.id.update({
        ...file,
        state: { tag: file.state.tag === 'Copying' ? 'Ready' : 'Deleted' },
        nextAttemptAt: later(tx.timestamp, config.receiptSeconds),
        lastError: undefined,
      });
    });
  }
  function sweepOrphans(ctx: Procedure<Tx>, provider: Service) {
    const claimed = ctx.withTx(tx => {
      const table = db(tx).cleanup,
        row = table.id.find(0)!;
      if (
        row.nextSweepAt.microsSinceUnixEpoch > tx.timestamp.microsSinceUnixEpoch
      )
        return;
      const next = {
        ...row,
        claim: row.claim + 1n,
        nextSweepAt: later(
          tx.timestamp,
          (PAGE_SIZE + 3) * config.httpTimeoutSeconds + 10
        ),
      };
      table.id.update(next);
      return next;
    });
    if (!claimed) return;
    let after = claimed.after,
      error: string | undefined;
    try {
      const page = provider.listFinal(after);
      for (const key of page.keys) {
        // Keys from an earlier installation or unknown files are orphans.
        const keep = ctx.withTx(tx => {
          const name = key.slice(`${config.prefix}final/`.length),
            install = `${installation(tx)}-`,
            id = name.startsWith(install)
              ? parseId(name.slice(install.length))
              : undefined,
            file = id === undefined ? undefined : db(tx).file.id.find(id);
          return file?.state.tag === 'Copying' || file?.state.tag === 'Ready';
        });
        if (!keep) provider.removeOrphan(key);
      }
      after = page.next;
    } catch (cause) {
      if (!(cause instanceof S3Error)) throw cause;
      error = cause.message;
    }
    ctx.withTx(tx => {
      const table = db(tx).cleanup;
      if (table.id.find(0)?.claim !== claimed.claim) return;
      table.id.update({
        ...claimed,
        after,
        lastError: error,
        // Continue a pass on the next call; wait only after a full pass or an error.
        nextSweepAt: later(
          tx.timestamp,
          error || !after ? config.sweepSeconds : 0
        ),
      });
    });
  }
  return {
    initialize(tx: Tx) {
      const table = tables(tx).cleanup;
      const existing = table.id.find(0);
      if (existing) {
        db(tx);
        return;
      }
      table.insert({
        id: 0,
        target,
        installedAt: tx.timestamp,
        after: '',
        nextSweepAt: tx.timestamp,
        claim: 0n,
        lastError: undefined,
      });
    },
    beginUpload(
      ctx: Procedure<Tx>,
      credentials: Credentials,
      args: UploadInput
    ) {
      const { owner, requestKey, filename, contentType, size, checksum } = args;
      if (
        !owner ||
        new TextEncoder().encode(owner).length > 256 ||
        !requestKey ||
        new TextEncoder().encode(requestKey).length > 128 ||
        !filename ||
        new TextEncoder().encode(filename).length > 255 ||
        Array.from(filename).some(ch => ch < ' ' || ch === '\x7f') ||
        contentType.length > 128 ||
        !/^[a-zA-Z0-9!#$&^_.+-]+\/[a-zA-Z0-9!#$&^_.+-]+$/.test(contentType) ||
        !Number.isSafeInteger(size) ||
        size < 0 ||
        size > config.maxFileBytes ||
        !/^[A-Za-z0-9+/]{43}=$/.test(checksum)
      )
        throw new SenderError('s3.invalid_metadata');
      try {
        base64ToBytes(checksum);
      } catch {
        throw new SenderError('s3.invalid_checksum');
      }
      const provider = service(ctx, credentials);
      provider.verifyBucket();
      const { file, open } = ctx.withTx(tx => {
        const tables = db(tx),
          key = JSON.stringify([owner, requestKey]);
        const existing = tables.file.requestKey.find(key);
        if (existing) {
          if (
            existing.filename !== filename ||
            existing.contentType !== contentType ||
            existing.size !== size ||
            existing.checksum !== checksum
          )
            throw new SenderError('s3.request_key_conflict');
          return {
            file: existing,
            open:
              existing.state.tag === 'Pending' &&
              existing.uploadExpiresAt.microsSinceUnixEpoch >
                tx.timestamp.microsSinceUnixEpoch,
          };
        }
        const usage = tables.ownerUsage.owner.find(owner) ?? {
          owner,
          count: 0,
          bytes: 0n,
        };
        if (
          usage.count >= config.maxFilesPerOwner ||
          usage.bytes + BigInt(size) > BigInt(config.maxBytesPerOwner)
        )
          throw new SenderError('s3.quota_exceeded');
        const expires = Math.min(
          millis(tx.timestamp) + config.uploadSeconds * 1000,
          credentialExpiry(credentials, millis(tx.timestamp))
        );
        const uploadExpiresAt = new Timestamp(BigInt(expires) * 1000n);
        const next = {
          ...usage,
          count: usage.count + 1,
          bytes: usage.bytes + BigInt(size),
        };
        if (usage.count) tables.ownerUsage.owner.update(next);
        else tables.ownerUsage.insert(next);
        const file = tables.file.insert({
          id: 0n,
          owner,
          requestKey: key,
          filename,
          contentType,
          size,
          checksum,
          state: { tag: 'Pending' },
          createdAt: tx.timestamp,
          uploadExpiresAt,
          nextAttemptAt: uploadExpiresAt,
          attempt: 0n,
          lastError: undefined,
        });
        return { file, open: true };
      });
      return {
        id: String(file.id),
        upload: open ? provider.upload(file) : undefined,
      };
    },
    completeUpload(
      ctx: Procedure<Tx>,
      credentials: Credentials,
      args: FileInput
    ) {
      const id = ctx.withTx(tx => {
        const file = owned(tx, args);
        if (file.state.tag === 'Ready' || file.state.tag === 'Copying')
          return file.id;
        if (
          file.state.tag !== 'Pending' ||
          file.uploadExpiresAt.microsSinceUnixEpoch <=
            tx.timestamp.microsSinceUnixEpoch
        )
          throw new SenderError('s3.upload_expired');
        db(tx).file.id.update({
          ...file,
          state: { tag: 'Copying' },
          nextAttemptAt: tx.timestamp,
        });
        return file.id;
      });
      processFile(ctx, service(ctx, credentials), id);
    },
    getDownloadUrl(
      ctx: Procedure<Tx>,
      credentials: Credentials,
      args: FileInput
    ) {
      const file = ctx.withTx(tx => {
        const file = owned(tx, args);
        if (file.state.tag !== 'Ready')
          throw new SenderError('s3.file_not_ready');
        return file;
      });
      return service(ctx, credentials).download(file);
    },
    deleteFile(tx: Tx, args: FileInput) {
      const file = owned(tx, args);
      if (file.state.tag !== 'Deleted' && file.state.tag !== 'Deleting')
        db(tx).file.id.update({
          ...file,
          state: { tag: 'Deleting' },
          nextAttemptAt: tx.timestamp,
          lastError: undefined,
        });
    },
    reconcile(ctx: Procedure<Tx>, credentials: Credentials) {
      const provider = service(ctx, credentials);
      for (const tag of [
        'Deleting',
        'Pending',
        'Copying',
        'Deleted',
      ] as const) {
        const id = ctx.withTx(
          tx =>
            db(tx)
              .file.byStateDue.filter([{ tag }, due(tx.timestamp)])
              .next().value?.id
        );
        if (id !== undefined) processFile(ctx, provider, id);
      }
      sweepOrphans(ctx, provider);
    },
  };
}
