import { moduleHooks } from 'spacetime:sys@2.0';
import { headersToList } from 'headers-polyfill';

import { AlgebraicType } from '../../lib/algebraic_type';
import BinaryReader from '../../lib/binary_reader';
import type { ConnectionId } from '../../lib/connection_id';
import { Identity } from '../../lib/identity';
import type { AuthCtx, JwtClaims, ReducerCtx } from '../../lib/reducers';
import type { UntypedSchemaDef } from '../../lib/schema';
import { Timestamp } from '../../lib/timestamp';
import type { TimeDuration } from '../../lib/time_duration';
import {
  type HttpRequest,
  type HttpResponse,
  type HttpMethod,
} from '../../lib/http_types';
import type { Table } from '../../lib/table';
import type { DbView } from '../db_view';
import {
  Headers,
  SyncResponse,
  type HttpClient,
  type RequestOptions,
} from '../http_internal';
import { makeTableView, ReducerCtxImpl } from '../runtime';
import type { ProcedureCtx } from '../procedures';
import { ProcedureCtxImpl } from '../procedures';
import type { Schema } from '../schema';
import type { AnonymousViewCtx, ViewCtx } from '../views';
import {
  makeQueryBuilder,
  getQueryAccessorName,
  toSql,
  type Query,
} from '../query';
import { NativeDatastoreBackend } from './backend';
import {
  loadNativeTestRuntime,
  type NativeContext,
  type NativeTx,
} from './native';

const freeze = Object.freeze;
const rawModuleDefCache = new WeakMap<
  Schema<any>,
  WeakMap<object, Uint8Array>
>();

export interface ModuleTestHarness<S extends UntypedSchemaDef> {
  readonly db: DbView<S>;
  readonly clock: TestClock;
  readonly rng: TestRng;
  readonly moduleIdentity: Identity;

  withReducerTx<T>(auth: TestAuth, body: (ctx: ReducerCtx<S>) => T): T;
  procedureContext(auth: TestAuth): ProcedureCtx<S>;
  procedureContextBuilder(auth: TestAuth): ProcedureContextBuilder<S>;
  viewContext(auth: TestAuth): ViewCtx<S>;
  anonymousViewContext(): AnonymousViewCtx<S>;
  runQuery<Row>(query: Query<any>): Row[];

  setHttpResponder(
    responder: (
      test: ModuleTestHarness<S>,
      req: HttpRequest,
      body: Uint8Array
    ) => HttpResponse
  ): void;
  reset(): void;
}

export class TestClock {
  #now: Timestamp;

  constructor(timestamp: Timestamp = Timestamp.UNIX_EPOCH) {
    this.#now = timestamp;
  }

  now(): Timestamp {
    return this.#now;
  }

  set(timestamp: Timestamp): void {
    this.#now = timestamp;
  }

  advance(duration: TimeDuration): void {
    this.#now = new Timestamp(this.#now.microsSinceUnixEpoch + duration.micros);
  }
}

export class TestRng {
  #seed: number | bigint | Uint8Array | null;

  constructor(seed: number | bigint | Uint8Array | null = null) {
    this.#seed = seed;
  }

  seed(): number | bigint | Uint8Array | null {
    return this.#seed;
  }

  setSeed(seed: number | bigint | Uint8Array): void {
    this.#seed = seed;
  }

  clearSeed(): void {
    this.#seed = null;
  }
}

class TestAuthImpl {
  private constructor(
    readonly kind: 'internal' | 'authenticated',
    readonly identity: Identity | null,
    readonly connectionId: ConnectionId | null,
    readonly jwtPayload: string | null
  ) {}

  static internal(identity?: Identity): TestAuthImpl {
    return new TestAuthImpl('internal', identity ?? null, null, null);
  }

  static fromJwtPayload(
    jwtPayload: string,
    connectionId: ConnectionId
  ): TestAuthImpl {
    return new TestAuthImpl('authenticated', null, connectionId, jwtPayload);
  }
}

export type TestAuth = TestAuthImpl;
export const TestAuth = TestAuthImpl;

export interface ProcedureContextBuilder<S extends UntypedSchemaDef> {
  http(
    responder: (
      test: ModuleTestHarness<S>,
      req: HttpRequest,
      body: Uint8Array
    ) => HttpResponse
  ): this;
  hooks(hooks: ProcedureTestHooks<S>): this;
  build(): ProcedureCtx<S>;
}

export class ProcedureTestHooks<S extends UntypedSchemaDef> {
  #afterTxCommit: Array<(test: ModuleTestHarness<S>) => void> = [];
  #onSleep: Array<(test: ModuleTestHarness<S>, wakeTime: Timestamp) => void> =
    [];

  afterTxCommit(hook: (test: ModuleTestHarness<S>) => void): this {
    this.#afterTxCommit.push(hook);
    return this;
  }

  onSleep(
    hook: (test: ModuleTestHarness<S>, wakeTime: Timestamp) => void
  ): this {
    this.#onSleep.push(hook);
    return this;
  }

  runAfterTxCommit(test: ModuleTestHarness<S>) {
    for (const hook of this.#afterTxCommit) hook(test);
  }

  runOnSleep(test: ModuleTestHarness<S>, wakeTime: Timestamp) {
    for (const hook of this.#onSleep) hook(test, wakeTime);
  }
}

export function createProcedureTestHooks<S extends UntypedSchemaDef>() {
  return new ProcedureTestHooks<S>();
}

export function createModuleTestHarness<S extends UntypedSchemaDef>(
  schema: Schema<S>,
  moduleExports: Record<string, unknown>,
  opts: {
    moduleIdentity?: Identity;
    clock?: TestClock;
    rngSeed?: bigint | number | null;
  } = {}
): ModuleTestHarness<S> {
  const rawModuleDef = describeModule(schema, moduleExports);
  const moduleIdentity = opts.moduleIdentity ?? Identity.zero();
  const native = loadNativeTestRuntime();
  const nativeContext = native.createContext(
    rawModuleDef,
    moduleIdentity.__identity__
  );

  return new ModuleTestHarnessImpl(
    schema,
    nativeContext,
    moduleIdentity,
    opts.clock ?? new TestClock(),
    new TestRng(opts.rngSeed ?? null)
  );
}

function describeModule<S extends UntypedSchemaDef>(
  schema: Schema<S>,
  moduleExports: Record<string, unknown>
): Uint8Array {
  let schemaCache = rawModuleDefCache.get(schema);
  if (!schemaCache) {
    schemaCache = new WeakMap<object, Uint8Array>();
    rawModuleDefCache.set(schema, schemaCache);
  }

  let rawModuleDef = schemaCache.get(moduleExports);
  if (!rawModuleDef) {
    const hooks = schema[moduleHooks](moduleExports);
    rawModuleDef = hooks.__describe_module__();
    schemaCache.set(moduleExports, rawModuleDef);
  }
  return rawModuleDef;
}

class ModuleTestHarnessImpl<S extends UntypedSchemaDef>
  implements ModuleTestHarness<S>
{
  readonly clock: TestClock;
  readonly rng: TestRng;
  readonly moduleIdentity: Identity;
  readonly db: DbView<S>;

  #schema: Schema<S>;
  #native: NativeContext;
  #backend: NativeDatastoreBackend;
  #httpResponder:
    | ((
        test: ModuleTestHarness<S>,
        req: HttpRequest,
        body: Uint8Array
      ) => HttpResponse)
    | undefined;

  constructor(
    schema: Schema<S>,
    native: NativeContext,
    moduleIdentity: Identity,
    clock: TestClock,
    rng: TestRng
  ) {
    this.#schema = schema;
    this.#native = native;
    this.moduleIdentity = moduleIdentity;
    this.clock = clock;
    this.rng = rng;
    this.#backend = new NativeDatastoreBackend(
      native,
      native,
      moduleIdentity.__identity__
    );
    this.db = makeDbView(schema, this.#backend);
  }

  withReducerTx<T>(auth: TestAuth, body: (ctx: ReducerCtx<S>) => T): T {
    const tx = this.#native.beginTx();
    const backend = this.#backend.withTransaction(tx);
    const sender = this.#sender(auth);
    if (auth.jwtPayload && auth.connectionId) {
      backend.setJwtPayload(
        auth.connectionId.__connection_id__,
        auth.jwtPayload
      );
    }

    try {
      const ctx = new ReducerCtxImpl(
        sender,
        this.clock.now(),
        auth.connectionId,
        makeDbView(this.#schema, backend),
        backend,
        this.#authCtx(auth, sender)
      );
      const ret = body(ctx as unknown as ReducerCtx<S>);
      this.#native.commitTx(tx, 'DropEventTableRows');
      return ret;
    } catch (e) {
      this.#native.abortTx(tx);
      throw e;
    }
  }

  procedureContext(auth: TestAuth): ProcedureCtx<S> {
    return this.procedureContextBuilder(auth).build();
  }

  procedureContextBuilder(auth: TestAuth): ProcedureContextBuilder<S> {
    return new ProcedureContextBuilderImpl(this, auth);
  }

  viewContext(auth: TestAuth): ViewCtx<S> {
    return freeze({
      sender: this.#sender(auth),
      db: this.db,
      from: makeQueryBuilder(this.#schema.schemaType),
    }) as ViewCtx<S>;
  }

  anonymousViewContext(): AnonymousViewCtx<S> {
    return freeze({
      db: this.db,
      from: makeQueryBuilder(this.#schema.schemaType),
    }) as AnonymousViewCtx<S>;
  }

  runQuery<Row>(query: Query<any>): Row[] {
    const sql = toSql(query);
    const accessorName = getQueryAccessorName(query);
    const table = this.#schema.schemaType.tables[accessorName];
    if (!table)
      throw new Error(`query source table not found: ${accessorName}`);
    const rowType = this.#schema.typespace.types[table.tableDef.productTypeRef];
    const rowDeserializer = AlgebraicType.makeDeserializer(
      rowType,
      this.#schema.typespace
    );
    return this.#native
      .runQuery(sql, this.moduleIdentity.__identity__)
      .map(row => rowDeserializer(new BinaryReader(row))) as Row[];
  }

  setHttpResponder(
    responder: (
      test: ModuleTestHarness<S>,
      req: HttpRequest,
      body: Uint8Array
    ) => HttpResponse
  ): void {
    this.#httpResponder = responder;
  }

  reset(): void {
    this.#native.reset();
  }

  makeProcedureContext(
    auth: TestAuth,
    hooks: ProcedureTestHooks<S>,
    responder:
      | ((
          test: ModuleTestHarness<S>,
          req: HttpRequest,
          body: Uint8Array
        ) => HttpResponse)
      | undefined
  ): ProcedureCtx<S> {
    const sender = this.#sender(auth);
    const backend = new ProcedureTestBackend(
      this,
      this.#native,
      this.#backend,
      this.moduleIdentity.__identity__,
      hooks
    );
    if (auth.jwtPayload && auth.connectionId) {
      backend.setJwtPayload(
        auth.connectionId.__connection_id__,
        auth.jwtPayload
      );
    }

    return new ProcedureCtxImpl(
      sender,
      this.clock.now(),
      auth.connectionId,
      () => makeDbView(this.#schema, backend.currentBackend()),
      backend,
      makeHttpClient(this, responder ?? this.#httpResponder),
      duration => this.#sleep(duration, hooks)
    ) as ProcedureCtx<S>;
  }

  #sender(auth: TestAuth): Identity {
    if (auth.kind === 'internal') return auth.identity ?? this.moduleIdentity;
    if (auth.identity) return auth.identity;
    if (!auth.jwtPayload || !auth.connectionId) {
      throw new Error(
        'authenticated test auth requires a JWT payload and connection id'
      );
    }
    const validated = this.#native.validateJwtPayload(
      auth.jwtPayload,
      auth.connectionId.__connection_id__
    );
    return new Identity(validated.senderHex);
  }

  #sleep(duration: TimeDuration, hooks: ProcedureTestHooks<S>): void {
    const wakeTime = new Timestamp(
      this.clock.now().microsSinceUnixEpoch + duration.micros
    );
    hooks.runOnSleep(this, wakeTime);
    if (this.clock.now().microsSinceUnixEpoch < wakeTime.microsSinceUnixEpoch) {
      this.clock.set(wakeTime);
    }
  }

  #authCtx(auth: TestAuth, sender: Identity): AuthCtx {
    if (auth.kind === 'internal') {
      return freeze({ isInternal: true, hasJWT: false, jwt: null });
    }
    const payload = JSON.parse(auth.jwtPayload!) as Record<string, unknown>;
    const jwt: JwtClaims = freeze({
      rawPayload: auth.jwtPayload!,
      subject: payload.sub as string,
      issuer: payload.iss as string,
      audience:
        payload.aud == null
          ? []
          : typeof payload.aud === 'string'
            ? [payload.aud]
            : (payload.aud as string[]),
      identity: sender,
      fullPayload: payload as any,
    });
    return freeze({ isInternal: false, hasJWT: true, jwt });
  }
}

class ProcedureContextBuilderImpl<S extends UntypedSchemaDef>
  implements ProcedureContextBuilder<S>
{
  #hooks = new ProcedureTestHooks<S>();
  #responder:
    | ((
        test: ModuleTestHarness<S>,
        req: HttpRequest,
        body: Uint8Array
      ) => HttpResponse)
    | undefined;

  constructor(
    private readonly test: ModuleTestHarnessImpl<S>,
    private readonly auth: TestAuth
  ) {}

  http(
    responder: (
      test: ModuleTestHarness<S>,
      req: HttpRequest,
      body: Uint8Array
    ) => HttpResponse
  ): this {
    this.#responder = responder;
    return this;
  }

  hooks(hooks: ProcedureTestHooks<S>): this {
    this.#hooks = hooks;
    return this;
  }

  build(): ProcedureCtx<S> {
    return this.test.makeProcedureContext(
      this.auth,
      this.#hooks,
      this.#responder
    );
  }
}

class ProcedureTestBackend<
  S extends UntypedSchemaDef,
> extends NativeDatastoreBackend {
  #ctx: NativeContext;
  #tx: NativeTx | null = null;

  constructor(
    private readonly test: ModuleTestHarness<S>,
    ctx: NativeContext,
    base: NativeDatastoreBackend,
    moduleIdentity: bigint,
    private readonly hooks: ProcedureTestHooks<S>
  ) {
    super(ctx, ctx, moduleIdentity);
    this.#ctx = ctx;
    void base;
    void hooks;
  }

  currentBackend(): NativeDatastoreBackend {
    return this.#tx ? this.withTransaction(this.#tx) : this;
  }

  procedureStartMutTx(): bigint {
    this.#tx = this.#ctx.beginTx();
    return this.test.clock.now().microsSinceUnixEpoch;
  }

  procedureCommitMutTx(): void {
    if (!this.#tx) throw new Error('no active procedure transaction');
    const tx = this.#tx;
    this.#tx = null;
    this.#ctx.commitTx(tx);
    this.hooks.runAfterTxCommit(this.test);
  }

  procedureAbortMutTx(): void {
    if (!this.#tx) return;
    const tx = this.#tx;
    this.#tx = null;
    this.#ctx.abortTx(tx);
  }
}

function makeDbView<S extends UntypedSchemaDef>(
  schema: Schema<S>,
  backend: NativeDatastoreBackend
): DbView<S> {
  return freeze(
    Object.fromEntries(
      Object.values(schema.schemaType.tables).map(table => [
        table.accessorName,
        makeTableView(schema.typespace, table.tableDef, backend) as Table<any>,
      ])
    )
  ) as DbView<S>;
}

function makeHttpClient<S extends UntypedSchemaDef>(
  test: ModuleTestHarness<S>,
  responder:
    | ((
        test: ModuleTestHarness<S>,
        req: HttpRequest,
        body: Uint8Array
      ) => HttpResponse)
    | undefined
): HttpClient {
  const methods = new Map<string, HttpMethod>([
    ['GET', { tag: 'Get' }],
    ['HEAD', { tag: 'Head' }],
    ['POST', { tag: 'Post' }],
    ['PUT', { tag: 'Put' }],
    ['DELETE', { tag: 'Delete' }],
    ['CONNECT', { tag: 'Connect' }],
    ['OPTIONS', { tag: 'Options' }],
    ['TRACE', { tag: 'Trace' }],
    ['PATCH', { tag: 'Patch' }],
  ]);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  return freeze({
    fetch(url: URL | string, init: RequestOptions = {}) {
      if (!responder) {
        throw new Error('no test HTTP responder configured');
      }
      const headers = new Headers(init.headers as any);
      const request: HttpRequest = freeze({
        method: methods.get(init.method?.toUpperCase() ?? 'GET') ?? {
          tag: 'Extension' as const,
          value: init.method!,
        },
        headers: {
          entries: headersToList(headers as any)
            .flatMap(([k, v]) =>
              Array.isArray(v) ? v.map(v => [k, v]) : [[k, v]]
            )
            .map(([name, value]) => ({
              name,
              value: encoder.encode(value),
            })),
        },
        timeout: init.timeout,
        uri: `${url}`,
        version: { tag: 'Http11' as const },
      });
      const body =
        init.body == null
          ? new Uint8Array()
          : typeof init.body === 'string'
            ? encoder.encode(init.body)
            : new Uint8Array(init.body as any);
      const response = responder(test, request, body);
      return new SyncResponse(null, {
        status: response.code,
        statusText: '',
        headers: new Headers(
          response.headers.entries.map(({ name, value }): [string, string] => [
            name,
            decoder.decode(value),
          ])
        ),
      });
    },
  });
}
