import { performance } from 'node:perf_hooks';
import { Identity } from '../../lib/identity';
import type { DbConnectionBuilder } from '../db_connection_builder';
import type { DbConnectionImpl } from '../db_connection_impl';
import {
  INTERNAL_MANAGED_SESSION,
  type ManagedSessionLifecycle,
} from '../managed_session_lifecycle';
import type { WebSocketAdapter } from '../ws';
import {
  Container,
  ContainerCredentialError,
  type ContainerToken,
} from './container';
import { openNodeWebSocket, type NodeWebSocketAdapter } from './index';

export type ContainerSessionErrorCode =
  | 'invalid_configuration'
  | 'already_running'
  | 'terminated'
  | 'denied'
  | 'credentials_failed'
  | 'sender_mismatch'
  | 'callback_failed'
  | 'message_processing_failed';
export class ContainerSessionError extends Error {
  constructor(readonly code: ContainerSessionErrorCode) {
    super(`Container session failed: ${code}`);
    this.name = 'ContainerSessionError';
  }
}
export interface ContainerGeneration {
  readonly generation: number;
  readonly target: Identity;
  readonly expiresAt: Date;
}
export type ContainerSessionEvent =
  | { readonly type: 'connected'; readonly generation: number }
  | {
      readonly type: 'disconnected';
      readonly generation: number;
      readonly reason:
        | 'renewed'
        | 'expired'
        | 'connection_closed'
        | 'paused'
        | 'shutdown'
        | 'failed';
      readonly unconfirmedCalls: 'unknown';
    }
  | { readonly type: 'retrying_credentials' };
export interface ContainerSessionOptions<C extends DbConnectionImpl<any>> {
  readonly container: Container;
  readonly target?: Identity;
  readonly serverUri?: string;
  /** Synchronous setup, called after sender verification for every fresh cache. */
  readonly onConnect: (connection: C, generation: ContainerGeneration) => void;
  readonly onEvent?: (event: ContainerSessionEvent) => void;
  readonly compression?: 'gzip' | 'brotli' | 'none';
  readonly lightMode?: boolean;
  readonly confirmedReads?: boolean;
}
type EndReason = Extract<
  ContainerSessionEvent,
  { type: 'disconnected' }
>['reason'];

class Wake {
  #listeners = new Set<() => void>();
  notify(): void {
    for (const listener of this.#listeners) listener();
  }
  async wait(milliseconds: number, signal: AbortSignal): Promise<void> {
    if (signal.aborted) return;
    await new Promise<void>(resolve => {
      const done = (): void => {
        clearTimeout(timer);
        this.#listeners.delete(done);
        signal.removeEventListener('abort', done);
        resolve();
      };
      const timer = setTimeout(done, Math.max(0, Math.min(milliseconds, 250)));
      this.#listeners.add(done);
      signal.addEventListener('abort', done, { once: true });
      if (signal.aborted) done();
    });
  }
}

/** Drops all late decompression/message callbacks once the generation is sealed. */
class ManagedSocket implements WebSocketAdapter {
  #sealed = false;
  #failure?: () => void;
  #closed?: () => void;
  constructor(
    private adapter: NodeWebSocketAdapter,
    failure: () => void,
    closed: () => void
  ) {
    this.#failure = failure;
    this.#closed = closed;
  }
  get protocol(): string {
    return this.adapter.protocol;
  }
  get readyState(): number {
    return this.adapter.readyState;
  }
  send(bytes: Uint8Array<ArrayBuffer>): void {
    if (this.#sealed) throw new ContainerSessionError('terminated');
    this.adapter.send(bytes);
  }
  seal(): void {
    this.#sealed = true;
  }
  close(): void {
    this.seal();
    this.adapter.close();
  }
  async shutdown(): Promise<void> {
    this.seal();
    await this.adapter.shutdown();
    this.#failure = undefined;
    this.#closed = undefined;
    this.adapter.onmessage = () => {};
    this.adapter.onopen = () => {};
    this.adapter.onerror = () => {};
    this.adapter.onclose = () => {};
  }
  set onopen(handler: () => void) {
    this.adapter.onopen = () => {
      if (!this.#sealed) {
        try {
          handler();
        } catch {
          this.#failure?.();
        }
      }
    };
  }
  set onmessage(handler: (message: { data: Uint8Array }) => void) {
    this.adapter.onmessage = message => {
      if (!this.#sealed) {
        try {
          handler(message);
        } catch {
          this.#failure?.();
        }
      }
    };
  }
  set onerror(handler: (event: ErrorEvent) => void) {
    this.adapter.onerror = event => {
      if (!this.#sealed) {
        try {
          handler(event);
        } catch {
          this.#failure?.();
        }
      }
    };
  }
  set onclose(handler: (event: CloseEvent) => void) {
    this.adapter.onclose = event => {
      this.seal();
      this.#closed?.();
      try {
        handler(event);
      } catch {
        this.#failure?.();
      }
    };
  }
}
interface Generation<C> {
  number: number;
  token: ContainerToken;
  connection?: C;
  lifecycle?: ManagedSessionLifecycle;
  socket?: ManagedSocket;
  socketPromise?: Promise<ManagedSocket>;
  wake: Wake;
  connected: boolean;
  closed: boolean;
  sealed: boolean;
  failure?: ContainerSessionError;
}

/**
 * An explicit owner of sequential Node database connections and credential renewal.
 * Pass the unmodified generated DbConnection class. Each generation gets its own
 * builder, callbacks, subscriptions and cache. run(signal) pauses by closing and
 * joining the current request/socket before resolving; a later run resumes with
 * fresh credentials. shutdown() is terminal. Always await one of these owners.
 * No reducer or procedure is replayed, and closure does not imply rollback.
 */
export class ContainerSession<C extends DbConnectionImpl<any>> {
  #builder: () => DbConnectionBuilder<C>;
  #options: ContainerSessionOptions<C>;
  #target: string;
  #sender: string;
  #server: string;
  #running?: Promise<void>;
  #abort?: AbortController;
  #terminal = false;
  #nextGeneration = 0;
  #active?: Generation<C>;
  #builders = new WeakSet<DbConnectionBuilder<C>>();
  #wake = new Wake();

  constructor(
    connectionType: { builder(): DbConnectionBuilder<C> },
    options: ContainerSessionOptions<C>
  ) {
    try {
      this.#builder = connectionType.builder.bind(connectionType);
      this.#options = { ...options };
      this.#target = (options.target ?? options.container.databaseIdentity)
        .toHexString()
        .toLowerCase();
      this.#sender = options.container.databaseIdentity
        .toHexString()
        .toLowerCase();
      if (
        !/^[0-9a-f]{64}$/.test(this.#target) ||
        !/^[0-9a-f]{64}$/.test(this.#sender) ||
        typeof options.onConnect !== 'function'
      )
        throw new Error();
      const value = options.serverUri ?? options.container.serverUri;
      if (
        !value ||
        Buffer.byteLength(value) > 4096 ||
        [...value].some(
          character =>
            character.charCodeAt(0) <= 32 ||
            character.charCodeAt(0) === 127 ||
            character === '?' ||
            character === '#'
        )
      )
        throw new Error();
      const url = new URL(value);
      if (
        !['http:', 'https:', 'ws:', 'wss:'].includes(url.protocol) ||
        !url.hostname ||
        url.username ||
        url.password
      )
        throw new Error();
      url.protocol = ['https:', 'wss:'].includes(url.protocol) ? 'wss:' : 'ws:';
      this.#server = url.href;
    } catch {
      throw new ContainerSessionError('invalid_configuration');
    }
  }

  run(signal?: AbortSignal): Promise<void> {
    if (this.#terminal)
      return Promise.reject(new ContainerSessionError('terminated'));
    if (this.#running)
      return Promise.reject(new ContainerSessionError('already_running'));
    const abort = new AbortController();
    this.#abort = abort;
    const pause = (): void => {
      if (this.#active) this.#seal(this.#active);
      abort.abort();
    };
    signal?.addEventListener('abort', pause, { once: true });
    if (signal?.aborted) pause();
    const running = this.#drive(abort.signal)
      .catch(error => {
        this.#terminal = true;
        throw error instanceof ContainerSessionError
          ? error
          : new ContainerSessionError('credentials_failed');
      })
      .finally(() => {
        signal?.removeEventListener('abort', pause);
        this.#abort = undefined;
        this.#running = undefined;
      });
    this.#running = running;
    return running;
  }

  async shutdown(): Promise<void> {
    this.#terminal = true;
    if (this.#active) this.#seal(this.#active);
    this.#abort?.abort();
    // run owns and reports its failure. shutdown still positively joins cleanup.
    await this.#running?.catch(() => {});
  }

  #invoke(callback: (() => void) | undefined): void {
    if (!callback) return;
    try {
      const result: unknown = callback();
      if (
        result &&
        typeof (result as PromiseLike<unknown>).then === 'function'
      ) {
        // Async application work cannot be cancelled or joined by the SDK.
        void Promise.resolve(result).catch(() => {});
        throw new Error();
      }
    } catch {
      throw new ContainerSessionError('callback_failed');
    }
  }
  #event(event: ContainerSessionEvent): void {
    this.#invoke(
      this.#options.onEvent &&
        (() => this.#options.onEvent!(Object.freeze(event)))
    );
  }
  #seal(generation: Generation<C>): void {
    if (generation.sealed) return;
    generation.sealed = true;
    generation.socket?.seal();
    try {
      generation.lifecycle?.seal(new ContainerSessionError('terminated'));
    } catch {
      generation.failure ??= new ContainerSessionError('callback_failed');
    }
    generation.wake.notify();
  }
  async #close(generation: Generation<C>, reason: EndReason): Promise<void> {
    this.#seal(generation);
    generation.connection?.disconnect();
    const socket =
      generation.socket ??
      (await generation.socketPromise?.catch(() => undefined));
    await socket?.shutdown();
    if (this.#active === generation) this.#active = undefined;
    this.#event({
      type: 'disconnected',
      generation: generation.number,
      reason,
      unconfirmedCalls: 'unknown',
    });
  }

  async #open(
    token: ContainerToken,
    signal: AbortSignal
  ): Promise<Generation<C>> {
    const generation: Generation<C> = {
      number: ++this.#nextGeneration,
      token,
      wake: new Wake(),
      connected: false,
      closed: false,
      sealed: false,
    };
    this.#active = generation;
    const fail = (code: ContainerSessionErrorCode): void => {
      generation.failure ??= new ContainerSessionError(code);
      this.#seal(generation);
    };
    try {
      const builder = this.#builder();
      if (this.#builders.has(builder))
        throw new ContainerSessionError('invalid_configuration');
      this.#builders.add(builder);
      const connection = builder
        .withUri(this.#server)
        .withDatabaseName(this.#target)
        .withToken(token.value)
        .withCompression(this.#options.compression ?? 'gzip')
        .withLightMode(this.#options.lightMode ?? false)
        .withWSFn(args => {
          const server = new URL(args.url);
          server.searchParams.delete('connection_id');
          if (
            server.href !== this.#server ||
            args.nameOrAddress !== this.#target ||
            args.authToken !== token.value
          )
            throw new ContainerSessionError('invalid_configuration');
          const promise = openNodeWebSocket(args).then(adapter => {
            const socket = new ManagedSocket(
              adapter,
              () => fail('message_processing_failed'),
              () => {
                generation.closed = true;
                generation.wake.notify();
              }
            );
            generation.socket = socket;
            if (generation.sealed || signal.aborted) socket.close();
            return socket;
          });
          generation.socketPromise = promise;
          return promise;
        })
        .onConnect((connected, identity) => {
          if (
            generation.sealed ||
            signal.aborted ||
            token.remainingLifetimeMs <= 0
          ) {
            this.#seal(generation);
            return;
          }
          if (identity.toHexString().toLowerCase() !== this.#sender) {
            fail('sender_mismatch');
            return;
          }
          if (generation.connected) {
            fail('message_processing_failed');
            return;
          }
          generation.connected = true;
          try {
            this.#invoke(() =>
              this.#options.onConnect(
                connected,
                Object.freeze({
                  generation: generation.number,
                  target: new Identity(this.#target),
                  expiresAt: token.expiresAt,
                })
              )
            );
            if (!generation.sealed)
              this.#event({ type: 'connected', generation: generation.number });
          } catch {
            fail('callback_failed');
          }
          generation.wake.notify();
        })
        .onConnectError(() => {
          generation.closed = true;
          generation.wake.notify();
        })
        .onDisconnect(() => {
          generation.closed = true;
          generation.wake.notify();
        });
      if (this.#options.confirmedReads !== undefined)
        connection.withConfirmedReads(this.#options.confirmedReads);
      generation.connection = connection.build();
      generation.lifecycle = generation.connection[INTERNAL_MANAGED_SESSION]();
      generation.lifecycle.enable();
      return generation;
    } catch (error) {
      await this.#close(generation, 'failed');
      throw error instanceof ContainerSessionError
        ? error
        : new ContainerSessionError('invalid_configuration');
    }
  }

  async #request(
    signal: AbortSignal,
    generation?: Generation<C>
  ): Promise<ContainerToken | undefined> {
    if (signal.aborted) return undefined;
    const abort = new AbortController();
    const cancel = (): void => abort.abort();
    signal.addEventListener('abort', cancel, { once: true });
    const wake = generation?.wake ?? this.#wake;
    let done = false;
    let value: ContainerToken | undefined;
    let failure: unknown;
    const pending = this.#options.container
      .tokenFor(new Identity(this.#target), abort.signal)
      .then(
        token => {
          value = token;
        },
        error => {
          failure = error;
        }
      )
      .finally(() => {
        done = true;
        wake.notify();
      });
    try {
      while (!done) {
        if (
          signal.aborted ||
          generation?.closed ||
          generation?.failure ||
          (generation && generation.token.remainingLifetimeMs <= 0)
        ) {
          if (generation) this.#seal(generation);
          abort.abort();
          await pending;
          return undefined;
        }
        await wake.wait(250, signal);
      }
      if (signal.aborted) return undefined;
      if (failure) throw failure;
      if (!value || value.target.toHexString() !== this.#target)
        throw new ContainerSessionError('credentials_failed');
      return value;
    } finally {
      signal.removeEventListener('abort', cancel);
      abort.abort();
      await pending;
    }
  }

  #transient(error: unknown): boolean {
    return (
      error instanceof ContainerCredentialError &&
      ['transport', 'timeout', 'unavailable'].includes(error.code)
    );
  }
  #credentialFailure(error: unknown): ContainerSessionError {
    return error instanceof ContainerCredentialError && error.code === 'denied'
      ? new ContainerSessionError('denied')
      : new ContainerSessionError('credentials_failed');
  }
  async #delay(
    milliseconds: number,
    signal: AbortSignal,
    generation?: Generation<C>
  ): Promise<void> {
    const until = performance.now() + milliseconds;
    while (
      !signal.aborted &&
      performance.now() < until &&
      !generation?.closed &&
      !generation?.failure &&
      !(generation && generation.token.remainingLifetimeMs <= 0)
    ) {
      await (generation?.wake ?? this.#wake).wait(
        until - performance.now(),
        signal
      );
    }
  }
  async #drive(signal: AbortSignal): Promise<void> {
    let next: ContainerToken | undefined;
    while (!signal.aborted) {
      if (!next || next.remainingLifetimeMs <= 0) {
        try {
          next = await this.#request(signal);
        } catch (error) {
          if (!this.#transient(error)) throw this.#credentialFailure(error);
          this.#event({ type: 'retrying_credentials' });
          await this.#delay(500, signal);
          continue;
        }
      }
      if (!next || signal.aborted) break;
      const generation = await this.#open(next, signal);
      next = undefined;
      let reason: EndReason = 'connection_closed';
      try {
        const openingDeadline = performance.now() + 5000;
        while (
          !generation.connected &&
          !generation.closed &&
          !generation.failure &&
          !signal.aborted &&
          generation.token.remainingLifetimeMs > 0 &&
          performance.now() < openingDeadline
        )
          await generation.wake.wait(250, signal);
        if (generation.failure) throw generation.failure;
        if (generation.connected) {
          let wait = Math.max(
            0,
            generation.token.remainingLifetimeMs -
              Math.min(5000, generation.token.remainingLifetimeMs / 3)
          );
          while (
            !signal.aborted &&
            !generation.closed &&
            !generation.failure &&
            generation.token.remainingLifetimeMs > 0
          ) {
            await this.#delay(wait, signal, generation);
            if (
              signal.aborted ||
              generation.closed ||
              generation.failure ||
              generation.token.remainingLifetimeMs <= 0
            )
              break;
            let refreshed: ContainerToken | undefined;
            try {
              refreshed = await this.#request(signal, generation);
            } catch (error) {
              if (!this.#transient(error)) throw this.#credentialFailure(error);
              this.#event({ type: 'retrying_credentials' });
            }
            if (
              refreshed &&
              refreshed.expiresAt.getTime() >=
                generation.token.expiresAt.getTime() + 1000 &&
              refreshed.remainingLifetimeMs >=
                generation.token.remainingLifetimeMs + 1000
            ) {
              next = refreshed;
              reason = 'renewed';
              break;
            }
            wait = 500;
          }
        }
        if (generation.failure) throw generation.failure;
        if (signal.aborted) reason = this.#terminal ? 'shutdown' : 'paused';
        else if (generation.token.remainingLifetimeMs <= 0) reason = 'expired';
      } catch (error) {
        reason = 'failed';
        throw error;
      } finally {
        await this.#close(generation, reason);
      }
      if (generation.failure) throw generation.failure;
      if (!next && !signal.aborted) await this.#delay(250, signal);
    }
  }
}
