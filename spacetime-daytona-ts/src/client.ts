import { Timestamp, type Identity } from 'spacetimedb';
import {
  Range,
  SenderError,
  type InferSchema,
  type ReducerCtx,
} from 'spacetimedb/server';
import submodule from './submodule.js';
import {
  api,
  shellCommand,
  ProviderError,
  type Http,
  type RemoteSandbox,
} from './api.js';
import type { Sandbox, Execution } from './model.js';

export type DaytonaTx = ReducerCtx<InferSchema<typeof submodule>>;
type HostTx = { timestamp: Timestamp; databaseIdentity: Identity };
type Procedure<Tx> = {
  sender: Identity;
  databaseIdentity: Identity;
  http: Http;
  withTx<T>(body: (tx: Tx) => T): T;
};

export interface DaytonaConfig<Tx> {
  submodule(tx: Tx): DaytonaTx;
  /** Distinguishes installations within the same database. */
  namespace: string;
  snapshot: string;
  ttlMinutes: number;
  maxSandboxesPerOwner?: number;
  /** HTTPS origins approved to receive the Daytona credential. */
  toolboxOrigins?: string[];
}

const SECOND = 1_000_000n;
const RETENTION = 86_400n * SECOND;
const MAX_SANDBOXES = 100;
const MAX_EXECUTIONS = 100;
const later = (now: Timestamp, delay: bigint) =>
  new Timestamp(now.microsSinceUnixEpoch + delay);
const due = (now: Timestamp) =>
  new Range<Timestamp>(null, { tag: 'included', value: now });
function fail(code: string): never {
  throw new SenderError(`daytona.${code}`);
}
function key(value: string): void {
  if (!value || new TextEncoder().encode(value).length > 128)
    fail('invalid_request_key');
}
function errorCode(error: unknown): string {
  return error instanceof ProviderError
    ? error.message
    : 'daytona.operation_failed';
}
function missing(error: unknown): boolean {
  return error instanceof ProviderError && error.status === 404;
}

/** Host reducers authorize callers and supply the owner key. */
export function client<Tx extends HostTx>(config: DaytonaConfig<Tx>) {
  const maxOwned = config.maxSandboxesPerOwner ?? 2;
  if (
    !/^[a-z][a-z0-9-]{0,15}$/.test(config.namespace) ||
    !config.snapshot ||
    !Number.isInteger(config.ttlMinutes) ||
    config.ttlMinutes < 1 ||
    config.ttlMinutes > 60 ||
    !Number.isInteger(maxOwned) ||
    maxOwned < 1 ||
    maxOwned > MAX_SANDBOXES
  ) {
    throw new Error('daytona.invalid_config');
  }
  const db = (tx: Tx) => config.submodule(tx).db;
  function owned(tx: Tx, owner: string, id: bigint): Sandbox {
    const row = db(tx).sandbox.id.find(id);
    if (!row || row.owner !== owner) fail('sandbox_not_found');
    return row;
  }

  function labels(ctx: { databaseIdentity: Identity }, row: Sandbox) {
    return {
      database: ctx.databaseIdentity.toHexString(),
      installation: config.namespace,
      request: row.remoteName,
    };
  }

  function verify(
    ctx: { databaseIdentity: Identity },
    row: Sandbox,
    remote: RemoteSandbox
  ): void {
    for (const [key, value] of Object.entries(labels(ctx, row))) {
      if (remote.labels[key] !== value)
        throw new ProviderError('ownership_mismatch');
    }
    if (row.remoteId !== undefined && row.remoteId !== remote.id)
      throw new ProviderError('ownership_mismatch');
  }

  function updateSandbox(
    ctx: Procedure<Tx>,
    claimed: Sandbox,
    patch: Partial<Sandbox>,
    delay = 10n * SECOND
  ) {
    ctx.withTx(tx => {
      const row = db(tx).sandbox.id.find(claimed.id);
      if (row?.claim !== claimed.claim) return;
      db(tx).sandbox.id.update({
        ...row,
        ...patch,
        checkAt: later(tx.timestamp, delay),
      });
    });
  }
  function updateExecution(
    ctx: Procedure<Tx>,
    claimed: Execution,
    patch: Partial<Execution>,
    finished = false
  ) {
    ctx.withTx(tx => {
      const row = db(tx).execution.id.find(claimed.id);
      if (row?.claim !== claimed.claim) return;
      db(tx).execution.id.update({
        ...row,
        ...patch,
        finishedAt: finished ? tx.timestamp : row.finishedAt,
        checkAt: later(tx.timestamp, finished ? RETENTION : 3n * SECOND),
      });
    });
  }

  function reconcileSandbox(
    ctx: Procedure<Tx>,
    service: ReturnType<typeof api>
  ): void {
    const claimed = ctx.withTx(tx => {
      const table = db(tx).sandbox;
      const row = table.checkAt.filter(due(tx.timestamp)).next().value;
      if (!row) return undefined;
      if (row.state.tag === 'Deleted') {
        // Executions have a per-sandbox bound. Remove them before the owner row.
        for (const execution of db(tx).execution.sandboxId.filter(row.id))
          db(tx).execution.id.delete(execution.id);
        table.id.delete(row.id);
        return undefined;
      }
      const claimed = {
        ...row,
        claim: row.claim + 1n,
        checkAt: later(tx.timestamp, 60n * SECOND),
      };
      table.id.update({
        ...claimed,
        state: row.state.tag === 'Queued' ? { tag: 'Creating' } : row.state,
      });
      return claimed;
    });
    if (!claimed) return;
    try {
      if (claimed.state.tag === 'Queued' && claimed.deleteRequested) {
        updateSandbox(
          ctx,
          claimed,
          { state: { tag: 'Deleted' }, error: undefined },
          RETENTION
        );
        return;
      }
      let remote: RemoteSandbox;
      try {
        remote =
          claimed.state.tag === 'Queued'
            ? service.create(
                claimed.remoteName,
                labels(ctx, claimed),
                claimed.snapshot,
                claimed.ttlMinutes
              )
            : service.sandbox(claimed.remoteId ?? claimed.remoteName);
      } catch (error) {
        if (missing(error) && claimed.remoteId !== undefined) {
          updateSandbox(
            ctx,
            claimed,
            { state: { tag: 'Deleted' }, error: undefined },
            RETENTION
          );
          return;
        }
        throw error;
      }
      verify(ctx, claimed, remote);
      const terminal =
        remote.state === 'destroyed' || remote.state === 'deleted';
      if (terminal) {
        updateSandbox(
          ctx,
          claimed,
          { remoteId: remote.id, state: { tag: 'Deleted' }, error: undefined },
          RETENTION
        );
        return;
      }
      if (claimed.deleteRequested) {
        service.delete(remote.id);
        updateSandbox(ctx, claimed, {
          remoteId: remote.id,
          state: { tag: 'Deleting' },
          error: undefined,
        });
        return;
      }
      const expiry =
        remote.autoDestroyAt === undefined
          ? NaN
          : Date.parse(remote.autoDestroyAt);
      updateSandbox(
        ctx,
        claimed,
        {
          remoteId: remote.id,
          expiresAt: Number.isFinite(expiry)
            ? new Timestamp(BigInt(expiry) * 1000n)
            : undefined,
          state: { tag: remote.state === 'started' ? 'Ready' : 'Unavailable' },
          error: undefined,
        },
        30n * SECOND
      );
    } catch (error) {
      updateSandbox(
        ctx,
        claimed,
        { state: { tag: 'Unknown' }, error: errorCode(error) },
        30n * SECOND
      );
    }
  }

  function reconcileExecution(
    ctx: Procedure<Tx>,
    service: ReturnType<typeof api>
  ): void {
    const work = ctx.withTx(tx => {
      const table = db(tx).execution;
      const row = table.checkAt.filter(due(tx.timestamp)).next().value;
      if (!row) return undefined;
      if (row.finishedAt) {
        table.id.delete(row.id);
        return undefined;
      }
      const sandbox = db(tx).sandbox.id.find(row.sandboxId);
      const claimed = {
        ...row,
        claim: row.claim + 1n,
        checkAt: later(tx.timestamp, 60n * SECOND),
      };
      table.id.update(claimed);
      return { row: claimed, sandbox };
    });
    if (!work) return;
    const { row, sandbox } = work;
    if (!sandbox || sandbox.state.tag === 'Deleted') {
      updateExecution(
        ctx,
        row,
        { state: { tag: 'Unknown' }, error: 'daytona.sandbox_deleted' },
        true
      );
      return;
    }
    if (sandbox.deleteRequested || !sandbox.remoteId) {
      updateExecution(ctx, row, {});
      return;
    }
    try {
      const remote = service.sandbox(sandbox.remoteId);
      verify(ctx, sandbox, remote);
      if (remote.state !== 'started') {
        updateExecution(ctx, row, { error: 'daytona.sandbox_not_ready' });
        return;
      }
      const base = service.toolbox(remote);
      if (row.state.tag === 'Queued') {
        try {
          const commands = service.session(base, row.sessionId);
          if (commands.length !== 0)
            throw new ProviderError('session_not_empty');
        } catch (error) {
          if (!missing(error)) throw error;
          service.createSession(base, row.sessionId);
        }
        // Only the worker which commits this transition may submit the command.
        const submit = ctx.withTx(tx => {
          const current = db(tx).execution.id.find(row.id);
          const parent = db(tx).sandbox.id.find(row.sandboxId);
          if (
            current?.claim !== row.claim ||
            current.state.tag !== 'Queued' ||
            !parent ||
            parent.deleteRequested ||
            parent.state.tag !== 'Ready'
          )
            return false;
          db(tx).execution.id.update({
            ...current,
            state: { tag: 'Submitting' },
          });
          return true;
        });
        if (!submit) return;
        const commandId = service.submit(base, row.sessionId, row.command);
        updateExecution(ctx, row, {
          commandId,
          state: { tag: 'Running' },
          error: undefined,
        });
        return;
      }
      const commands = row.commandId
        ? [service.command(base, row.sessionId, row.commandId)]
        : service.session(base, row.sessionId);
      if (
        commands.length !== 1 ||
        commands[0].command !== shellCommand(row.command) ||
        (row.commandId !== undefined && commands[0].id !== row.commandId)
      ) {
        throw new ProviderError('execution_unknown');
      }
      const command = commands[0];
      const finished = command.exitCode !== undefined;
      updateExecution(
        ctx,
        row,
        {
          commandId: command.id,
          exitCode: command.exitCode,
          error: undefined,
          state: {
            tag: finished
              ? command.exitCode === 0
                ? 'Succeeded'
                : 'Failed'
              : 'Running',
          },
        },
        finished
      );
    } catch (error) {
      ctx.withTx(tx => {
        const current = db(tx).execution.id.find(row.id);
        if (current?.claim !== row.claim) return;
        db(tx).execution.id.update({
          ...current,
          state:
            current.state.tag === 'Queued' ? current.state : { tag: 'Unknown' },
          error: errorCode(error),
          checkAt: later(tx.timestamp, 30n * SECOND),
        });
      });
    }
  }

  return {
    createSandbox(
      tx: Tx,
      input: { owner: string; requestKey: string }
    ): bigint {
      key(input.owner);
      key(input.requestKey);
      const table = db(tx).sandbox;
      const request = JSON.stringify([input.owner, input.requestKey]);
      const existing = table.request.find(request);
      if (existing) return existing.id;
      if (table.count() >= MAX_SANDBOXES) fail('history_full');
      let active = 0;
      for (const row of table.owner.filter(input.owner))
        if (row.state.tag !== 'Deleted') active++;
      if (active >= maxOwned) fail('sandbox_limit');
      const row = table.insert({
        id: 0n,
        owner: input.owner,
        request,
        remoteName: '',
        snapshot: config.snapshot,
        ttlMinutes: config.ttlMinutes,
        remoteId: undefined,
        state: { tag: 'Queued' },
        deleteRequested: false,
        expiresAt: undefined,
        claim: 0n,
        checkAt: tx.timestamp,
        error: undefined,
        createdAt: tx.timestamp,
      });
      table.id.update({
        ...row,
        remoteName: `stdb-${tx.databaseIdentity.toHexString().slice(0, 12)}-${config.namespace}-${tx.timestamp.microsSinceUnixEpoch.toString(36)}-${row.id.toString(36)}`,
      });
      return row.id;
    },
    runCommand(
      tx: Tx,
      input: {
        owner: string;
        sandboxId: bigint;
        requestKey: string;
        command: string;
      }
    ): bigint {
      const sandbox = owned(tx, input.owner, input.sandboxId);
      key(input.requestKey);
      if (
        !input.command.trim() ||
        new TextEncoder().encode(input.command).length > 4096
      )
        fail('invalid_command');
      const table = db(tx).execution;
      const request = JSON.stringify([String(sandbox.id), input.requestKey]);
      const existing = table.request.find(request);
      if (existing) {
        if (existing.command !== input.command) fail('request_key_conflict');
        return existing.id;
      }
      if (sandbox.deleteRequested || sandbox.state.tag !== 'Ready')
        fail('sandbox_not_ready');
      let count = 0;
      for (const row of table.sandboxId.filter(sandbox.id)) {
        if (!row.finishedAt) fail('command_pending');
        count++;
      }
      if (count >= MAX_EXECUTIONS) fail('history_full');
      const row = table.insert({
        id: 0n,
        sandboxId: sandbox.id,
        request,
        command: input.command,
        sessionId: '',
        commandId: undefined,
        state: { tag: 'Queued' },
        exitCode: undefined,
        claim: 0n,
        checkAt: tx.timestamp,
        error: undefined,
        finishedAt: undefined,
      });
      table.id.update({ ...row, sessionId: `stdb-${row.id}` });
      return row.id;
    },
    deleteSandbox(tx: Tx, input: { owner: string; sandboxId: bigint }): void {
      const row = owned(tx, input.owner, input.sandboxId);
      if (row.state.tag === 'Deleted' || row.deleteRequested) return;
      db(tx).sandbox.id.update({
        ...row,
        deleteRequested: true,
        claim: row.claim + 1n,
        checkAt: tx.timestamp,
      });
    },
    reconcile(ctx: Procedure<Tx>, apiKey: string): void {
      if (!ctx.sender.equals(ctx.databaseIdentity)) fail('scheduler_only');
      const service = api(ctx.http, apiKey, config.toolboxOrigins);
      reconcileSandbox(ctx, service);
      reconcileExecution(ctx, service);
    },
  };
}
