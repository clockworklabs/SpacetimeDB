import * as _syscalls2_0 from 'spacetime:sys@2.0';
import * as _syscalls2_1 from 'spacetime:sys@2.1';

import type { ModuleHooks, u128, u16, u256, u32 } from 'spacetime:sys@2.0';
import {
  AlgebraicType,
  ProductType,
  type Deserializer,
} from '../lib/algebraic_type';
import {
  RawModuleDef,
  ViewResultHeader,
  type RawProcedureDefV10,
  type RawReducerDefV10,
  type RawTableDefV10,
  type Typespace,
} from '../lib/autogen/types';
import { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import { Timestamp } from '../lib/timestamp';
import { Uuid } from '../lib/uuid';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter, { ResizableBuffer } from '../lib/binary_writer';
import {
  type Index,
  type IndexVal,
  type PointIndex,
  type RangedIndex,
  type UniqueIndex,
} from '../lib/indexes';
import { callProcedure } from './procedures';
import type { Procedures } from './procedures';
import type { Reducers } from './reducers';
import {
  type HandlerContext,
  Request,
  SyncResponse,
  makeRequest,
} from './http_handlers';
import { httpClient } from './http_internal';
import {
  deserializeHeaders,
  deserializeMethod,
  serializeHeaders,
} from './http_shared';
import {
  type AliasViews,
  type AuthCtx,
  type JsonObject,
  type JwtClaims,
  type ReducerCtx as IReducerCtx,
} from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import {
  type RowType,
  type Table,
  type TableMethods,
  type UntypedTableDef,
} from '../lib/table';
import { bsatnBaseSize, hasOwn } from '../lib/util';
import {
  type AnonymousViewCtx,
  type AnonViews,
  type ViewCtx,
  type Views,
} from './views';
import {
  isRowTypedQuery,
  makeQueryBuilder,
  toSql,
  type QueryBuilder,
} from './query';
import type { DbView, ReadonlyDbView } from './db_view';
import { getErrorConstructor, SenderError } from './errors';
import { Range, type Bound } from './range';
import { makeRandom, type Random } from './rng';
import type { SubmoduleDispatchInfo, SchemaInner } from './schema';
import { HttpRequest, HttpResponse } from '../lib/autogen/types';

const { freeze } = Object;

export const sys = { ..._syscalls2_0, ..._syscalls2_1 };

function requestFromWire(request: HttpRequest, body: Uint8Array): Request {
  return Request[makeRequest](body, {
    headers: deserializeHeaders(request.headers),
    method: deserializeMethod(request.method),
    uri: request.uri,
    version: request.version,
  });
}

function responseIntoWire(response: SyncResponse): [HttpResponse, Uint8Array] {
  return [
    {
      headers: serializeHeaders(response.headers),
      version: response.version,
      code: response.status,
    },
    response.bytes(),
  ];
}

export function parseJsonObject(json: string): JsonObject {
  let value: unknown;

  try {
    value = JSON.parse(json);
  } catch {
    throw new Error('Invalid JSON: failed to parse string');
  }

  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Expected a JSON object at the top level');
  }

  // The runtime check above guarantees this cast is safe
  return value as JsonObject;
}

class JwtClaimsImpl implements JwtClaims {
  readonly fullPayload: JsonObject;
  private readonly _identity: Identity;
  /**
   * Creates a new JwtClaims instance.
   * @param rawPayload The JWT payload as a raw JSON string.
   * @param identity The identity for this JWT. We are only taking this because we don't have a blake3 implementation (which we need to compute it).
   */
  constructor(
    public readonly rawPayload: string,
    identity: Identity
  ) {
    this.fullPayload = parseJsonObject(rawPayload);
    this._identity = identity;
  }
  readonly [claim: string]: unknown;
  get identity(): Identity {
    return this._identity;
  }
  get subject() {
    return this.fullPayload['sub'] as string;
  }
  get issuer() {
    return this.fullPayload['iss'] as string;
  }
  get audience() {
    const aud = this.fullPayload['aud'];
    if (aud == null) {
      return [];
    }
    return typeof aud === 'string' ? [aud] : (aud as string[]);
  }
}

class AuthCtxImpl implements AuthCtx {
  public readonly isInternal: boolean;

  // Source of the JWT payload string, if there is one.
  private readonly _jwtSource: () => string | null;
  // Whether we have initialized the JWT claims.
  private _initializedJWT: boolean = false;
  private _jwtClaims?: JwtClaims | null;
  private _senderIdentity: Identity;

  private constructor(opts: {
    isInternal: boolean;
    jwtSource: () => string | null;
    senderIdentity: Identity;
  }) {
    this.isInternal = opts.isInternal;
    this._jwtSource = opts.jwtSource;
    this._senderIdentity = opts.senderIdentity;
  }

  private _initializeJWT() {
    if (this._initializedJWT) return;
    this._initializedJWT = true;

    const token = this._jwtSource();
    if (!token) {
      this._jwtClaims = null;
    } else {
      this._jwtClaims = new JwtClaimsImpl(token, this._senderIdentity);
    }
    // At this point we can safely freeze the object.
    Object.freeze(this);
  }

  /** Lazily compute whether a JWT exists and is parseable. */
  get hasJWT(): boolean {
    this._initializeJWT();
    return this._jwtClaims !== null;
  }

  /** Lazily parse the JwtClaims only when accessed. */
  get jwt(): JwtClaims | null {
    this._initializeJWT();
    return this._jwtClaims!;
  }

  /** Create a context representing internal (non-user) requests. */
  static internal(): AuthCtx {
    return new AuthCtxImpl({
      isInternal: true,
      jwtSource: () => null,
      senderIdentity: Identity.zero(),
    });
  }

  /** If there is a connection id, look up the JWT payload from the system tables. */
  static fromSystemTables(
    connectionId: ConnectionId | null,
    sender: Identity
  ): AuthCtx {
    if (connectionId === null) {
      return new AuthCtxImpl({
        isInternal: false,
        jwtSource: () => null,
        senderIdentity: sender,
      });
    }
    return new AuthCtxImpl({
      isInternal: false,
      jwtSource: () => {
        const payloadBuf = sys.get_jwt_payload(connectionId.__connection_id__);
        if (payloadBuf.length === 0) return null;
        const payloadStr = new TextDecoder().decode(payloadBuf);
        return payloadStr;
      },
      senderIdentity: sender,
    });
  }
}

// Using a class expression rather than declaration keeps the class out of the
// type namespace, so that `ReducerCtx` still refers to the interface.
export const ReducerCtxImpl = class ReducerCtx<
  SchemaDef extends UntypedSchemaDef,
> implements IReducerCtx<SchemaDef>
{
  #identity: Identity | undefined;
  #senderAuth: AuthCtx | undefined;
  #uuidCounter: { value: number } | undefined;
  #random: Random | undefined;
  sender: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
  db: DbView<SchemaDef>;
  as: AliasViews<SchemaDef>;

  constructor(
    sender: Identity,
    timestamp: Timestamp,
    connectionId: ConnectionId | null,
    dbView: DbView<any>,
    asViews: object = {}
  ) {
    Object.seal(this);
    this.sender = sender;
    this.timestamp = timestamp;
    this.connectionId = connectionId;
    this.db = dbView as unknown as DbView<SchemaDef>;
    this.as = asViews as AliasViews<SchemaDef>;
  }

  /** Reset the `ReducerCtx` to be used for a new transaction */
  static reset(
    me: InstanceType<typeof this>,
    sender: Identity,
    timestamp: Timestamp,
    connectionId: ConnectionId | null,
    dbView?: DbView<any>,
    asViews?: object
  ) {
    me.sender = sender;
    me.timestamp = timestamp;
    me.connectionId = connectionId;
    me.#uuidCounter = undefined;
    me.#senderAuth = undefined;
    if (dbView !== undefined) {
      me.db = dbView;
    }
    if (asViews !== undefined) {
      me.as = asViews as AliasViews<any>;
    }
  }

  get databaseIdentity() {
    return (this.#identity ??= new Identity(sys.identity()));
  }

  get identity() {
    return this.databaseIdentity;
  }

  get senderAuth() {
    return (this.#senderAuth ??= AuthCtxImpl.fromSystemTables(
      this.connectionId,
      this.sender
    ));
  }

  get random() {
    return (this.#random ??= makeRandom(this.timestamp));
  }

  /**
   * Create a new random {@link Uuid} `v4` using this `ReducerCtx`'s RNG.
   */
  newUuidV4(): Uuid {
    const bytes = this.random.fill(new Uint8Array(16));
    return Uuid.fromRandomBytesV4(bytes);
  }

  /**
   * Create a new sortable {@link Uuid} `v7` using this `ReducerCtx`'s RNG, counter,
   * and timestamp.
   */
  newUuidV7(): Uuid {
    const bytes = this.random.fill(new Uint8Array(4));
    const counter = (this.#uuidCounter ??= { value: 0 });
    return Uuid.fromCounterV7(counter, this.timestamp, bytes);
  }
};

/**
 * Call into a user function `fn` - the backtrace from an exception thrown in
 * `fn` or one of its descendants in the callgraph will be stripped by host
 * code in `crates/core/src/host/v8/error.rs` such that `fn` will be shown to
 * be the root of the call stack.
 */
export const callUserFunction = function __spacetimedb_end_short_backtrace<
  Args extends any[],
  R,
>(fn: (...args: Args) => R, ...args: Args): R {
  return fn(...args);
};

export function runWithTx<T, Ctx>(
  makeCtx: (timestamp: Timestamp) => Ctx,
  body: (ctx: Ctx) => T
): T {
  const run = () => {
    const timestamp = sys.procedure_start_mut_tx();

    try {
      return body(makeCtx(new Timestamp(timestamp)));
    } catch (e) {
      sys.procedure_abort_mut_tx();
      throw e;
    }
  };

  let res = run();
  try {
    sys.procedure_commit_mut_tx();
    return res;
  } catch {
    // ignore the commit error
  }
  console.warn('committing anonymous transaction failed');
  res = run();
  try {
    sys.procedure_commit_mut_tx();
    return res;
  } catch (e) {
    throw new Error('transaction retry failed again', { cause: e });
  }
}

type FlatSubmoduleDispatch = {
  reducerFns: Reducers;
  reducerDefs: RawReducerDefV10[];
  procedureFns: Procedures;
  procedureDefs: RawProcedureDefV10[];
  anonViewFns: AnonViews;
  viewFns: Views;
  tables: Array<{ accessorName: string; tableDef: RawTableDefV10 }>;
  schemaTables: Record<string, UntypedTableDef>;
  typespace: Typespace;
  dbView_: DbView<any> | undefined;
  queryBuilder_: QueryBuilder<any> | undefined;
  /** e.g. "alias." for a submodule with namespace alias "alias" */
  namePrefix: string;
  subDispatches: SubmoduleDispatchInfo[];
};

function flattenSubmoduleDispatches(
  dispatches: SubmoduleDispatchInfo[],
  parentPrefix = ''
): FlatSubmoduleDispatch[] {
  const result: FlatSubmoduleDispatch[] = [];
  for (const d of dispatches) {
    const namePrefix = parentPrefix + d.namespace + '.';
    result.push({
      reducerFns: d.reducerFns,
      reducerDefs: d.reducerDefs,
      procedureFns: d.procedureFns,
      procedureDefs: d.procedureDefs,
      anonViewFns: d.anonViewFns,
      viewFns: d.viewFns,
      tables: d.tables,
      schemaTables: d.schemaTables,
      typespace: d.typespace,
      dbView_: undefined,
      queryBuilder_: undefined,
      namePrefix,
      subDispatches: d.subDispatches,
    });
    result.push(...flattenSubmoduleDispatches(d.subDispatches, namePrefix));
  }
  return result;
}

export const makeHooks = (schema: SchemaInner): ModuleHooks =>
  new ModuleHooksImpl(schema);

class ModuleHooksImpl implements ModuleHooks {
  #schema: SchemaInner;
  #dbView_: DbView<any> | undefined;
  #consumerAs_: object | undefined;
  #reducerArgsDeserializers;
  #consumerReducerCount: number;
  #consumerProcedureCount: number;
  #flatSubmodules: FlatSubmoduleDispatch[];
  #consumerAnonViewCount: number;
  #consumerViewCount: number;
  /** Cache the `ReducerCtx` object to avoid allocating anew for every reducer call. */
  #reducerCtx_: InstanceType<typeof ReducerCtxImpl> | undefined;
  /** Per-submodule alias ctx maps, cached lazily (parallel to #flatSubmodules). */
  #submoduleAsViews_: (object | undefined)[] = [];

  constructor(schema: SchemaInner) {
    this.#schema = schema;
    this.#consumerReducerCount = schema.reducers.length;
    this.#consumerProcedureCount = schema.procedures.length;
    this.#consumerAnonViewCount = schema.anonViews.length;
    this.#consumerViewCount = schema.views.length;
    this.#flatSubmodules = flattenSubmoduleDispatches(
      schema.submoduleDispatchInfos
    );

    const consumerDeserializers = schema.moduleDef.reducers.map(({ params }) =>
      ProductType.makeDeserializer(params, schema.typespace)
    );
    const submoduleDeserializers = this.#flatSubmodules.flatMap(
      ({ reducerDefs, typespace }) =>
        reducerDefs.map(({ params }) =>
          ProductType.makeDeserializer(params, typespace)
        )
    );
    this.#reducerArgsDeserializers = [
      ...consumerDeserializers,
      ...submoduleDeserializers,
    ];
  }

  get #dbView() {
    if (this.#dbView_ !== undefined) return this.#dbView_;
    const rootTables = Object.values(this.#schema.schemaType.tables).map(
      table => [
        table.accessorName,
        makeTableView(this.#schema.typespace, table.tableDef),
      ]
    );
    const submoduleNs = this.#schema.submoduleDispatchInfos.map(dispatch => [
      dispatch.namespace,
      buildDbViewForDispatch(dispatch, dispatch.namespace + '.'),
    ]);
    this.#dbView_ = freeze(
      Object.fromEntries([...rootTables, ...submoduleNs])
    ) as DbView<any>;
    return this.#dbView_;
  }

  #getSubmoduleDbView(submoduleIdx: number): DbView<any> {
    const m = this.#flatSubmodules[submoduleIdx];
    return (m.dbView_ ??= freeze(
      Object.fromEntries(
        m.tables.map(({ accessorName, tableDef }) => [
          accessorName,
          makeTableView(m.typespace, tableDef, m.namePrefix),
        ])
      ) as DbView<any>
    ));
  }

  #getSubmoduleAsViews(submoduleIdx: number): object {
    return (this.#submoduleAsViews_[submoduleIdx] ??= buildAliasCtxMap(
      this.#reducerCtx,
      this.#flatSubmodules[submoduleIdx].subDispatches,
      this.#flatSubmodules[submoduleIdx].namePrefix
    ));
  }

  /** Query builder scoped to the submodule's own tables, with namespace-prefixed
   *  source names so generated SQL targets the mounted table names. */
  #getSubmoduleQueryBuilder(submoduleIdx: number): QueryBuilder<any> {
    const m = this.#flatSubmodules[submoduleIdx];
    return (m.queryBuilder_ ??= makeQueryBuilder({
      tables: Object.fromEntries(
        Object.entries(m.schemaTables).map(([key, def]) => [
          key,
          { ...def, sourceName: m.namePrefix + def.sourceName },
        ])
      ),
    }));
  }

  get #reducerCtx() {
    return (this.#reducerCtx_ ??= new ReducerCtxImpl(
      Identity.zero(),
      Timestamp.UNIX_EPOCH,
      null,
      this.#dbView
    ));
  }

  get #consumerAs() {
    return (this.#consumerAs_ ??= buildAliasCtxMap(
      this.#reducerCtx,
      this.#schema.submoduleDispatchInfos,
      ''
    ));
  }

  __describe_module__() {
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(
      writer,
      RawModuleDef.V10(this.#schema.rawModuleDefV10())
    );
    return writer.getBuffer();
  }

  __get_error_constructor__(code: number): new (msg: string) => Error {
    return getErrorConstructor(code);
  }

  get __sender_error_class__() {
    return SenderError;
  }

  __call_reducer__(
    reducerId: u32,
    sender: u256,
    connId: u128,
    timestamp: bigint,
    argsBuf: DataView
  ): void {
    const deserializeArgs = this.#reducerArgsDeserializers[reducerId];
    BINARY_READER.reset(argsBuf);
    const args = deserializeArgs(BINARY_READER);
    const senderIdentity = new Identity(sender);

    let fn: ((...args: any[]) => any) | undefined;
    let dbView: DbView<any>;
    let asViews: object;

    if (reducerId < this.#consumerReducerCount) {
      fn = this.#schema.reducers[reducerId];
      dbView = this.#dbView;
      asViews = this.#consumerAs;
    } else {
      let offset = this.#consumerReducerCount;
      for (let i = 0; i < this.#flatSubmodules.length; i++) {
        const m = this.#flatSubmodules[i];
        if (reducerId < offset + m.reducerFns.length) {
          fn = m.reducerFns[reducerId - offset];
          dbView = this.#getSubmoduleDbView(i);
          asViews = this.#getSubmoduleAsViews(i);
          break;
        }
        offset += m.reducerFns.length;
      }
      if (fn === undefined) {
        throw new RangeError(`unknown reducerId ${reducerId}`);
      }
    }

    const ctx = this.#reducerCtx;
    ReducerCtxImpl.reset(
      ctx,
      senderIdentity,
      new Timestamp(timestamp),
      ConnectionId.nullIfZero(new ConnectionId(connId)),
      dbView!,
      asViews!
    );
    callUserFunction(fn, ctx, args);
  }

  __call_view__(
    id: u32,
    sender: u256,
    argsBuf: Uint8Array
  ): { data: Uint8Array } {
    const moduleCtx = this.#schema;
    let viewFns: Views;
    let localId: number;
    let dbView: ReadonlyDbView<any>;
    let from: QueryBuilder<any>;

    if (id < this.#consumerViewCount) {
      viewFns = moduleCtx.views;
      localId = id;
      dbView = this.#dbView as ReadonlyDbView<any>;
      from = makeQueryBuilder(moduleCtx.schemaType);
    } else {
      let offset = this.#consumerViewCount;
      let found = false;
      for (let i = 0; i < this.#flatSubmodules.length; i++) {
        const m = this.#flatSubmodules[i];
        if (id < offset + m.viewFns.length) {
          viewFns = m.viewFns;
          localId = id - offset;
          dbView = this.#getSubmoduleDbView(i) as ReadonlyDbView<any>;
          from = this.#getSubmoduleQueryBuilder(i);
          found = true;
          break;
        }
        offset += m.viewFns.length;
      }
      if (!found) throw new RangeError(`unknown viewId ${id}`);
    }

    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      viewFns![localId!];
    const ctx: ViewCtx<any> = freeze({
      sender: new Identity(sender),
      db: dbView!,
      from: from!,
    });
    const args = deserializeParams(new BinaryReader(argsBuf));
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RawSql(query));
    } else {
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RowData);
      serializeReturn(retBuf, ret);
    }
    return { data: retBuf.getBuffer() };
  }

  __call_view_anon__(id: u32, argsBuf: Uint8Array): { data: Uint8Array } {
    const moduleCtx = this.#schema;
    let anonViewFns: AnonViews;
    let localId: number;
    let dbView: ReadonlyDbView<any>;
    let from: QueryBuilder<any>;

    if (id < this.#consumerAnonViewCount) {
      anonViewFns = moduleCtx.anonViews;
      localId = id;
      dbView = this.#dbView as ReadonlyDbView<any>;
      from = makeQueryBuilder(moduleCtx.schemaType);
    } else {
      let offset = this.#consumerAnonViewCount;
      let found = false;
      for (let i = 0; i < this.#flatSubmodules.length; i++) {
        const m = this.#flatSubmodules[i];
        if (id < offset + m.anonViewFns.length) {
          anonViewFns = m.anonViewFns;
          localId = id - offset;
          dbView = this.#getSubmoduleDbView(i) as ReadonlyDbView<any>;
          from = this.#getSubmoduleQueryBuilder(i);
          found = true;
          break;
        }
        offset += m.anonViewFns.length;
      }
      if (!found) throw new RangeError(`unknown anonViewId ${id}`);
    }

    const { fn, deserializeParams, serializeReturn, returnTypeBaseSize } =
      anonViewFns![localId!];
    const ctx: AnonymousViewCtx<any> = freeze({
      db: dbView!,
      from: from!,
    });
    const args = deserializeParams(new BinaryReader(argsBuf));
    const ret = callUserFunction(fn, ctx, args);
    const retBuf = new BinaryWriter(returnTypeBaseSize);
    if (isRowTypedQuery(ret)) {
      const query = toSql(ret);
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RawSql(query));
    } else {
      ViewResultHeader.serialize(retBuf, ViewResultHeader.RowData);
      serializeReturn(retBuf, ret);
    }
    return { data: retBuf.getBuffer() };
  }

  __call_procedure__(
    id: u32,
    sender: u256,
    connection_id: u128,
    timestamp: bigint,
    args: Uint8Array
  ): Uint8Array {
    const senderIdentity = new Identity(sender);
    const connId = ConnectionId.nullIfZero(new ConnectionId(connection_id));
    const ts = new Timestamp(timestamp);

    if (id < this.#consumerProcedureCount) {
      return callProcedure(
        this.#schema.procedures,
        id,
        senderIdentity,
        connId,
        ts,
        args,
        () => this.#dbView as DbView<any>,
        this.#schema.submoduleDispatchInfos
      );
    }

    let offset = this.#consumerProcedureCount;
    for (let i = 0; i < this.#flatSubmodules.length; i++) {
      const m = this.#flatSubmodules[i];
      if (id < offset + m.procedureFns.length) {
        return callProcedure(
          m.procedureFns,
          id - offset,
          senderIdentity,
          connId,
          ts,
          args,
          () => this.#getSubmoduleDbView(i),
          m.subDispatches,
          m.namePrefix
        );
      }
      offset += m.procedureFns.length;
    }

    throw new RangeError(`unknown procedureId ${id}`);
  }

  __call_http_handler__(
    id: u32,
    timestamp: bigint,
    request: Uint8Array,
    body: Uint8Array
  ): [response: Uint8Array, body: Uint8Array] {
    const moduleCtx = this.#schema;
    const handler = moduleCtx.httpHandlers[id];
    const ctx = new HandlerContextImpl(
      new Timestamp(timestamp),
      () => this.#dbView,
      this.#schema.submoduleDispatchInfos
    );
    const requestMetadata = HttpRequest.deserialize(new BinaryReader(request));
    const response = callUserFunction(
      handler,
      ctx,
      requestFromWire(requestMetadata, body)
    );
    const [responseMetadata, responseBody] = responseIntoWire(response);
    const responseBuf = new BinaryWriter(
      bsatnBaseSize(moduleCtx.typespace, HttpResponse.algebraicType)
    );
    HttpResponse.serialize(responseBuf, responseMetadata);
    return [responseBuf.getBuffer(), responseBody];
  }
}

const BINARY_WRITER = new BinaryWriter(0);
const BINARY_READER = new BinaryReader(new Uint8Array());

class HandlerContextImpl<S extends UntypedSchemaDef = UntypedSchemaDef>
  implements HandlerContext<S>
{
  #identity: Identity | undefined;
  #uuidCounter: { value: number } | undefined;
  #random: Random | undefined;
  #dbView: () => DbView<any>;
  #dispatches: SubmoduleDispatchInfo[];
  #asViews: object | undefined;

  readonly http = httpClient;

  constructor(
    readonly timestamp: Timestamp,
    dbView: () => DbView<any>,
    dispatches: SubmoduleDispatchInfo[] = []
  ) {
    this.#dbView = dbView;
    this.#dispatches = dispatches;
  }

  get identity() {
    return (this.#identity ??= new Identity(sys.identity()));
  }

  get random() {
    return (this.#random ??= makeRandom(this.timestamp));
  }

  get as() {
    return (this.#asViews ??= buildHandlerAliasCtxMap(
      this,
      this.#dispatches,
      ''
    )) as any;
  }

  withTx<T>(body: (ctx: any) => T): T {
    const dispatches = this.#dispatches;
    return runWithTx(timestamp => {
      const tx = new ReducerCtxImpl(
        Identity.zero(),
        timestamp,
        null,
        this.#dbView()
      );
      if (dispatches.length > 0) {
        tx.as = buildAliasCtxMap(tx, dispatches, '') as any;
      }
      return tx;
    }, body);
  }

  newUuidV4(): Uuid {
    const bytes = this.random.fill(new Uint8Array(16));
    return Uuid.fromRandomBytesV4(bytes);
  }

  newUuidV7(): Uuid {
    const bytes = this.random.fill(new Uint8Array(4));
    const counter = (this.#uuidCounter ??= { value: 0 });
    return Uuid.fromCounterV7(counter, this.timestamp, bytes);
  }
}

function buildDbViewForDispatch(
  dispatch: SubmoduleDispatchInfo,
  namePrefix: string
): object {
  const tableEntries = dispatch.tables.map(({ accessorName, tableDef }) => [
    accessorName,
    makeTableView(dispatch.typespace, tableDef, namePrefix),
  ]);
  const subNsEntries = dispatch.subDispatches.map(sub => [
    sub.namespace,
    buildDbViewForDispatch(sub, namePrefix + sub.namespace + '.'),
  ]);
  return freeze(Object.fromEntries([...tableEntries, ...subNsEntries]));
}

function buildAliasCtx(
  parent: InstanceType<typeof ReducerCtxImpl>,
  dispatch: SubmoduleDispatchInfo,
  namePrefix: string
): object {
  const nsDb = buildDbViewForDispatch(dispatch, namePrefix);
  const subAs = buildAliasCtxMap(parent, dispatch.subDispatches, namePrefix);
  return {
    get sender() {
      return parent.sender;
    },
    get databaseIdentity() {
      return parent.databaseIdentity;
    },
    get identity() {
      return parent.identity;
    },
    get timestamp() {
      return parent.timestamp;
    },
    get connectionId() {
      return parent.connectionId;
    },
    get senderAuth() {
      return parent.senderAuth;
    },
    get random() {
      return parent.random;
    },
    newUuidV4() {
      return parent.newUuidV4();
    },
    newUuidV7() {
      return parent.newUuidV7();
    },
    db: nsDb,
    as: subAs,
  };
}

function buildAliasCtxMap(
  parent: InstanceType<typeof ReducerCtxImpl>,
  dispatches: SubmoduleDispatchInfo[],
  parentPrefix: string
): object {
  return freeze(
    Object.fromEntries(
      dispatches.map(d => [
        d.namespace,
        buildAliasCtx(parent, d, parentPrefix + d.namespace + '.'),
      ])
    )
  );
}

function buildHandlerAliasCtx(
  parent: HandlerContextImpl,
  dispatch: SubmoduleDispatchInfo,
  namePrefix: string
): object {
  // nsDb is built lazily inside withTx so that sys.table_id_from_name is called
  // only after a transaction has been started (sys.procedure_start_mut_tx).
  let nsDb_: DbView<any> | undefined;
  const subAs = buildHandlerAliasCtxMap(
    parent,
    dispatch.subDispatches,
    namePrefix
  );
  return {
    get timestamp() {
      return parent.timestamp;
    },
    get http() {
      return parent.http;
    },
    get identity() {
      return parent.identity;
    },
    get random() {
      return parent.random;
    },
    as: subAs,
    withTx(body: any) {
      return runWithTx((ts: Timestamp) => {
        const tx = new ReducerCtxImpl(
          Identity.zero(),
          ts,
          null,
          (nsDb_ ??= buildDbViewForDispatch(
            dispatch,
            namePrefix
          ) as DbView<any>)
        );
        assignTxAliasViews(tx, dispatch.subDispatches, namePrefix);
        return tx;
      }, body);
    },
    newUuidV4() {
      return parent.newUuidV4();
    },
    newUuidV7() {
      return parent.newUuidV7();
    },
  };
}

function buildHandlerAliasCtxMap(
  parent: HandlerContextImpl,
  dispatches: SubmoduleDispatchInfo[],
  parentPrefix: string
): object {
  return freeze(
    Object.fromEntries(
      dispatches.map(d => [
        d.namespace,
        buildHandlerAliasCtx(parent, d, parentPrefix + d.namespace + '.'),
      ])
    )
  );
}

type ProcCtxRef = {
  sender: Identity;
  connectionId: ConnectionId | null;
  timestamp: Timestamp;
  get databaseIdentity(): Identity;
  get identity(): Identity;
  get http(): typeof httpClient;
  get random(): Random;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
};

function buildProcedureAliasCtx(
  parent: ProcCtxRef,
  dispatch: SubmoduleDispatchInfo,
  namePrefix: string
): object {
  // nsDb is built lazily inside withTx so that sys.table_id_from_name is called
  // only after a transaction has been started (sys.procedure_start_mut_tx).
  let nsDb_: DbView<any> | undefined;
  const subAs = buildProcedureAliasCtxMap(
    parent,
    dispatch.subDispatches,
    namePrefix
  );
  return {
    get sender() {
      return parent.sender;
    },
    get databaseIdentity() {
      return parent.databaseIdentity;
    },
    get identity() {
      return parent.identity;
    },
    get timestamp() {
      return parent.timestamp;
    },
    get connectionId() {
      return parent.connectionId;
    },
    get http() {
      return parent.http;
    },
    get random() {
      return parent.random;
    },
    as: subAs,
    withTx(body: any) {
      return runWithTx((ts: Timestamp) => {
        const tx = new ReducerCtxImpl(
          parent.sender,
          ts,
          parent.connectionId,
          (nsDb_ ??= buildDbViewForDispatch(
            dispatch,
            namePrefix
          ) as DbView<any>)
        );
        assignTxAliasViews(tx, dispatch.subDispatches, namePrefix);
        return tx;
      }, body);
    },
    newUuidV4() {
      return parent.newUuidV4();
    },
    newUuidV7() {
      return parent.newUuidV7();
    },
  };
}

export function buildProcedureAliasCtxMap(
  parent: ProcCtxRef,
  dispatches: SubmoduleDispatchInfo[],
  parentPrefix: string
): object {
  return freeze(
    Object.fromEntries(
      dispatches.map(d => [
        d.namespace,
        buildProcedureAliasCtx(parent, d, parentPrefix + d.namespace + '.'),
      ])
    )
  );
}

/** Builds and assigns reducer-style alias views onto a freshly created TransactionCtx.
 *  Must be called while inside a transaction (after sys.procedure_start_mut_tx). */
export function assignTxAliasViews(
  tx: InstanceType<typeof ReducerCtxImpl>,
  dispatches: SubmoduleDispatchInfo[],
  parentPrefix = ''
): void {
  if (dispatches.length > 0) {
    tx.as = buildAliasCtxMap(tx, dispatches, parentPrefix) as any;
  }
}

export function makeTableView(
  typespace: Typespace,
  table: RawTableDefV10,
  namePrefix = ''
): Table<any> {
  const table_id = sys.table_id_from_name(namePrefix + table.sourceName);
  const rowType = typespace.types[table.productTypeRef];
  if (rowType.tag !== 'Product') {
    throw 'impossible';
  }

  const serializeRow = AlgebraicType.makeSerializer(rowType, typespace);
  const deserializeRow = AlgebraicType.makeDeserializer(rowType, typespace);

  const sequences = table.sequences.map(seq => {
    const col = rowType.value.elements[seq.column];
    const colType = col.algebraicType;

    // Determine the sentinel value which users will pass to as a placeholder
    // to cause the sequence to advance.
    // For small integer SATS types which fit in V8 `number`s, this is `0: number`,
    // and for larger integer SATS types it's `0n: BigInt`.
    let sequenceTrigger: bigint | number;
    switch (colType.tag) {
      case 'U8':
      case 'I8':
      case 'U16':
      case 'I16':
      case 'U32':
      case 'I32':
        sequenceTrigger = 0;
        break;
      case 'U64':
      case 'I64':
      case 'U128':
      case 'I128':
      case 'U256':
      case 'I256':
        sequenceTrigger = 0n;
        break;
      default:
        throw new TypeError('invalid sequence type');
    }
    return {
      colName: col.name!,
      sequenceTrigger,
      deserialize: AlgebraicType.makeDeserializer(colType, typespace),
    };
  });
  const hasAutoIncrement = sequences.length > 0;

  const iter = () =>
    tableIterator(sys.datastore_table_scan_bsatn(table_id), deserializeRow);

  const integrateGeneratedColumns = hasAutoIncrement
    ? (row: RowType<any>, ret_buf: DataView) => {
        BINARY_READER.reset(ret_buf);
        for (const { colName, deserialize, sequenceTrigger } of sequences) {
          if (row[colName] === sequenceTrigger) {
            row[colName] = deserialize(BINARY_READER);
          }
        }
      }
    : null;

  const tableMethods: TableMethods<any> = {
    count: () => sys.datastore_table_row_count(table_id),
    iter,
    [Symbol.iterator]: () => iter(),
    insert: row => {
      const buf = LEAF_BUF;
      BINARY_WRITER.reset(buf);
      serializeRow(BINARY_WRITER, row);
      sys.datastore_insert_bsatn(table_id, buf.buffer, BINARY_WRITER.offset);
      const ret = { ...row };
      integrateGeneratedColumns?.(ret, buf.view);

      return ret;
    },
    delete: (row: RowType<any>): boolean => {
      const buf = LEAF_BUF;
      BINARY_WRITER.reset(buf);
      BINARY_WRITER.writeU32(1);
      serializeRow(BINARY_WRITER, row);
      const count = sys.datastore_delete_all_by_eq_bsatn(
        table_id,
        buf.buffer,
        BINARY_WRITER.offset
      );
      return count > 0;
    },
    clear: () => sys.datastore_clear(table_id),
  };

  const tableView = Object.assign(
    Object.create(null),
    tableMethods
  ) as Table<any>;

  for (const indexDef of table.indexes) {
    const accessorName = indexDef.accessorName!;
    const index_id = sys.index_id_from_name(namePrefix + indexDef.sourceName!);

    let column_ids: number[];
    let isHashIndex = false;
    switch (indexDef.algorithm.tag) {
      case 'Hash':
        isHashIndex = true;
        column_ids = indexDef.algorithm.value;
        break;
      case 'BTree':
        column_ids = indexDef.algorithm.value;
        break;
      case 'Direct':
        column_ids = [indexDef.algorithm.value];
        break;
    }
    const numColumns = column_ids.length;

    const columnSet = new Set(column_ids);
    const isUnique = table.constraints
      .filter(x => x.data.tag === 'Unique')
      .some(x => columnSet.isSubsetOf(new Set(x.data.value.columns)));

    const isPrimaryKey =
      isUnique &&
      column_ids.length === table.primaryKey.length &&
      column_ids.every((id, i) => table.primaryKey[i] === id);

    const indexSerializers = column_ids.map(id =>
      AlgebraicType.makeSerializer(
        rowType.value.elements[id].algebraicType,
        typespace
      )
    );

    const serializePoint = (buffer: ResizableBuffer, colVal: any[]): number => {
      BINARY_WRITER.reset(buffer);
      for (let i = 0; i < numColumns; i++) {
        indexSerializers[i](BINARY_WRITER, colVal[i]);
      }
      return BINARY_WRITER.offset;
    };

    const serializeSingleElement =
      numColumns === 1 ? indexSerializers[0] : null;

    const serializeSinglePoint =
      serializeSingleElement &&
      ((buffer: ResizableBuffer, colVal: any): number => {
        BINARY_WRITER.reset(buffer);
        serializeSingleElement(BINARY_WRITER, colVal);
        return BINARY_WRITER.offset;
      });

    type IndexScanArgs = [
      prefix_len: u32,
      prefix_elems: u16,
      rstart_len: u32,
      rend_len: u32,
    ];

    let index: Index<any, any>;
    if (isUnique && serializeSinglePoint) {
      // numColumns == 1, unique index
      const base = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, colVal);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          const buf = LEAF_BUF;
          const point_len = serializeSinglePoint(buf, colVal);
          const num = sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return num > 0;
        },
      };
      if (isPrimaryKey) {
        (base as any).update = (row: RowType<any>): RowType<any> => {
          const buf = LEAF_BUF;
          BINARY_WRITER.reset(buf);
          serializeRow(BINARY_WRITER, row);
          sys.datastore_update_bsatn(
            table_id,
            index_id,
            buf.buffer,
            BINARY_WRITER.offset
          );
          integrateGeneratedColumns?.(row, buf.view);
          return row;
        };
      }
      index = base as UniqueIndex<any, any>;
    } else if (isUnique) {
      // numColumns != 1, unique index
      const base = {
        find: (colVal: IndexVal<any, any>): RowType<any> | null => {
          if (colVal.length !== numColumns) {
            throw new TypeError('wrong number of elements');
          }
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, colVal);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterateOne(iter_id, deserializeRow);
        },
        delete: (colVal: IndexVal<any, any>): boolean => {
          if (colVal.length !== numColumns)
            throw new TypeError('wrong number of elements');

          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, colVal);
          const num = sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return num > 0;
        },
      };
      if (isPrimaryKey) {
        (base as any).update = (row: RowType<any>): RowType<any> => {
          const buf = LEAF_BUF;
          BINARY_WRITER.reset(buf);
          serializeRow(BINARY_WRITER, row);
          sys.datastore_update_bsatn(
            table_id,
            index_id,
            buf.buffer,
            BINARY_WRITER.offset
          );
          integrateGeneratedColumns?.(row, buf.view);
          return row;
        };
      }
      index = base as UniqueIndex<any, any>;
    } else if (serializeSinglePoint) {
      // numColumns == 1

      const serializeSingleRange = !isHashIndex
        ? (buffer: ResizableBuffer, range: Range<any>): IndexScanArgs => {
            BINARY_WRITER.reset(buffer);
            const writer = BINARY_WRITER;
            const writeBound = (bound: Bound<any>) => {
              const tags = { included: 0, excluded: 1, unbounded: 2 };
              writer.writeU8(tags[bound.tag]);
              if (bound.tag !== 'unbounded')
                serializeSingleElement!(writer, bound.value);
            };
            writeBound(range.from);
            const rstartLen = writer.offset;
            writeBound(range.to);
            const rendLen = writer.offset - rstartLen;
            return [0, 0, rstartLen, rendLen];
          }
        : null;

      const rawIndex = {
        filter: (range: any): IteratorObject<RowType<any>> => {
          const buf = LEAF_BUF;
          if (serializeSingleRange && range instanceof Range) {
            const args = serializeSingleRange(buf, range);
            const iter_id = sys.datastore_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
            return tableIterator(iter_id, deserializeRow);
          }
          const point_len = serializeSinglePoint(buf, range);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterator(iter_id, deserializeRow);
        },
        delete: (range: any): u32 => {
          const buf = LEAF_BUF;
          if (serializeSingleRange && range instanceof Range) {
            const args = serializeSingleRange(buf, range);
            return sys.datastore_delete_by_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
          }
          const point_len = serializeSinglePoint(buf, range);
          return sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
        },
      };
      if (isHashIndex) {
        index = rawIndex as PointIndex<any, any>;
      } else {
        index = rawIndex as RangedIndex<any, any>;
      }
    } else if (isHashIndex) {
      // numColumns != 1
      index = {
        filter: (range: any[]): IteratorObject<RowType<any>> => {
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, range);
          const iter_id = sys.datastore_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
          return tableIterator(iter_id, deserializeRow);
        },
        delete: (range: any[]): u32 => {
          const buf = LEAF_BUF;
          const point_len = serializePoint(buf, range);
          return sys.datastore_delete_by_index_scan_point_bsatn(
            index_id,
            buf.buffer,
            point_len
          );
        },
      } as PointIndex<any, any>;
    } else {
      // numColumns != 1
      const serializeRange = (
        buffer: ResizableBuffer,
        range: any[]
      ): IndexScanArgs => {
        if (range.length > numColumns) throw new TypeError('too many elements');

        BINARY_WRITER.reset(buffer);
        const writer = BINARY_WRITER;
        const prefix_elems = range.length - 1;
        for (let i = 0; i < prefix_elems; i++) {
          indexSerializers[i](writer, range[i]);
        }
        const rstartOffset = writer.offset;
        const term = range[range.length - 1];
        const serializeTerm = indexSerializers[range.length - 1];
        if (term instanceof Range) {
          const writeBound = (bound: Bound<any>) => {
            const tags = { included: 0, excluded: 1, unbounded: 2 };
            writer.writeU8(tags[bound.tag]);
            if (bound.tag !== 'unbounded') serializeTerm(writer, bound.value);
          };
          writeBound(term.from);
          const rstartLen = writer.offset - rstartOffset;
          writeBound(term.to);
          const rendLen = writer.offset - rstartLen;
          return [rstartOffset, prefix_elems, rstartLen, rendLen];
        } else {
          writer.writeU8(0);
          serializeTerm(writer, term);
          const rstartLen = writer.offset;
          const rendLen = 0;
          return [rstartOffset, prefix_elems, rstartLen, rendLen];
        }
      };
      index = {
        filter: (range: any[]): IteratorObject<RowType<any>> => {
          // A bare scalar or `Range` is the only type-valid way to express a
          // one-column prefix scan; normalize it to a single-element array so
          // `.length` and `serializeRange` see a prefix rather than NaN.
          if (!Array.isArray(range)) range = [range];
          if (range.length === numColumns) {
            const buf = LEAF_BUF;
            const point_len = serializePoint(buf, range);
            const iter_id = sys.datastore_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
            return tableIterator(iter_id, deserializeRow);
          } else {
            const buf = LEAF_BUF;
            const args = serializeRange(buf, range);
            const iter_id = sys.datastore_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
            return tableIterator(iter_id, deserializeRow);
          }
        },
        delete: (range: any[]): u32 => {
          if (!Array.isArray(range)) range = [range];
          if (range.length === numColumns) {
            const buf = LEAF_BUF;
            const point_len = serializePoint(buf, range);
            return sys.datastore_delete_by_index_scan_point_bsatn(
              index_id,
              buf.buffer,
              point_len
            );
          } else {
            const buf = LEAF_BUF;
            const args = serializeRange(buf, range);
            return sys.datastore_delete_by_index_scan_range_bsatn(
              index_id,
              buf.buffer,
              ...args
            );
          }
        },
      } as RangedIndex<any, any>;
    }

    // IMPORTANT: duplicate accessor handling.
    // When multiple raw indexes share the same accessor name, we merge index
    // methods onto a single accessor object instead of throwing.
    if (Object.hasOwn(tableView, accessorName)) {
      freeze(Object.assign((tableView as any)[accessorName], index));
    } else {
      (tableView as any)[accessorName] = freeze(index);
    }
  }

  return freeze(tableView);
}

function* tableIterator<T>(
  id: u32,
  deserialize: Deserializer<T>
): Generator<T, undefined> {
  using iter = new IteratorHandle(id);

  const iterBuf = takeBuf();
  try {
    let amt;
    while ((amt = iter.advance(iterBuf))) {
      const reader = new BinaryReader(iterBuf.view);
      while (reader.offset < amt) {
        yield deserialize(reader);
      }
    }
  } finally {
    returnBuf(iterBuf);
  }
}

function tableIterateOne<T>(id: u32, deserialize: Deserializer<T>): T | null {
  const buf = LEAF_BUF;
  // we only need to check for the `<= 0` case, since this function is only used
  // with iterators that should only have zero or one element.
  const ret = advanceIterRaw(id, buf);
  if (ret !== 0) {
    BINARY_READER.reset(buf.view);
    return deserialize(BINARY_READER);
  }
  return null;
}

/**
 * `ret < 0` means the iterator yielded elements but is now exhausted and has been destroyed.
 * `ret === 0` means the iterator was empty and has been destroyed.
 * `ret > 0` means the iterator yielded elements and has more to give.
 */
function advanceIterRaw(id: u32, buf: ResizableBuffer): number {
  while (true) {
    try {
      return 0 | sys.row_iter_bsatn_advance(id, buf.buffer);
    } catch (e) {
      if (e && typeof e === 'object' && hasOwn(e, '__buffer_too_small__')) {
        buf.grow(e.__buffer_too_small__ as number);
        continue;
      }
      throw e;
    }
  }
}

// This should guarantee in most cases that we don't have to reallocate an iterator
// buffer, unless there's a single row that serializes to >1 MiB.
const DEFAULT_BUFFER_CAPACITY = 32 * 1024 * 2;

const ITER_BUFS: ResizableBuffer[] = [
  new ResizableBuffer(DEFAULT_BUFFER_CAPACITY),
];
let ITER_BUF_COUNT = 1;

function takeBuf(): ResizableBuffer {
  return ITER_BUF_COUNT
    ? ITER_BUFS[--ITER_BUF_COUNT]
    : new ResizableBuffer(DEFAULT_BUFFER_CAPACITY);
}

function returnBuf(buf: ResizableBuffer) {
  ITER_BUFS[ITER_BUF_COUNT++] = buf;
}

/**
 * This should only be used from functions that don't need persistent ownership
 * over the buffer. While using this value, one should not call a function that
 * also uses this value.
 */
const LEAF_BUF = new ResizableBuffer(DEFAULT_BUFFER_CAPACITY);

/** A class to manage the lifecycle of an iterator handle. */
class IteratorHandle implements Disposable {
  #id: u32 | -1;

  static #finalizationRegistry = new FinalizationRegistry<u32>(
    sys.row_iter_bsatn_close
  );

  constructor(id: u32) {
    this.#id = id;
    IteratorHandle.#finalizationRegistry.register(this, id, this);
  }

  /** Unregister this object with the finalization registry and return the id */
  #detach() {
    const id = this.#id;
    this.#id = -1;
    IteratorHandle.#finalizationRegistry.unregister(this);
    return id;
  }

  /** Call `row_iter_bsatn_advance`, returning 0 if this iterator has been exhausted. */
  advance(buf: ResizableBuffer): number {
    if (this.#id === -1) return 0;
    const ret = advanceIterRaw(this.#id, buf);
    if (ret <= 0) this.#detach();
    return ret < 0 ? -ret : ret;
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      const id = this.#detach();
      sys.row_iter_bsatn_close(id);
    }
  }
}
