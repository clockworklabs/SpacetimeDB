import type {
  NativeCommitMode,
  NativeContext,
  NativeTarget,
  NativeTestRuntime,
  NativeTx,
} from './native';

export type WasmCommitMode = 'Normal' | 'DropEventTableRows';

export interface WasmValidatedAuth {
  readonly senderHex: string;
  readonly connectionIdHex: string | undefined;
}

export interface WasmPortableTransaction extends NativeTx {
  readonly __wasmTxBrand?: unique symbol;
}

export interface WasmPortableDatastore {
  tableId(name: string): number;
  indexId(name: string): number;
  beginMutTx(): WasmPortableTransaction;
  commitTx(tx: WasmPortableTransaction, mode: WasmCommitMode): void;
  rollbackTx(tx: WasmPortableTransaction): void;
  reset(): void;

  tableRowCount(tableId: number): number;
  tableRowCountTx(tx: WasmPortableTransaction, tableId: number): number;
  tableRowsBsatn(tableId: number): Uint8Array[];
  tableRowsBsatnTx(tx: WasmPortableTransaction, tableId: number): Uint8Array[];
  indexScanPointBsatn(indexId: number, point: Uint8Array): Uint8Array[];
  indexScanPointBsatnTx(
    tx: WasmPortableTransaction,
    indexId: number,
    point: Uint8Array
  ): Uint8Array[];
  indexScanRangeBsatn(
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): Uint8Array[];
  indexScanRangeBsatnTx(
    tx: WasmPortableTransaction,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): Uint8Array[];

  insertBsatnGeneratedCols(
    tx: WasmPortableTransaction,
    tableId: number,
    row: Uint8Array
  ): Uint8Array;
  updateBsatnGeneratedCols(
    tx: WasmPortableTransaction,
    tableId: number,
    indexId: number,
    row: Uint8Array
  ): Uint8Array;
  deleteByRelBsatn(
    tx: WasmPortableTransaction,
    tableId: number,
    relation: Uint8Array
  ): number;
  deleteByIndexScanPointBsatn(
    tx: WasmPortableTransaction,
    indexId: number,
    point: Uint8Array
  ): number;
  deleteByIndexScanRangeBsatn(
    tx: WasmPortableTransaction,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): number;
  clearTable(tx: WasmPortableTransaction, tableId: number): number;

  validateJwtPayload(
    payload: string,
    connectionIdHex: string
  ): WasmValidatedAuth;
  runQuery(sql: string, databaseIdentityHex: string): Uint8Array[];
}

export interface WasmPortableDatastoreModule {
  WasmPortableDatastore: new (
    rawModuleDefBsatn: Uint8Array,
    moduleIdentityHex: string
  ) => WasmPortableDatastore;
}

export function createWasmTestRuntime(
  wasm: WasmPortableDatastoreModule
): NativeTestRuntime {
  return {
    createContext(moduleDef, moduleIdentity) {
      return new WasmContext(
        new wasm.WasmPortableDatastore(
          moduleDef,
          bigintToFixedHex(moduleIdentity, 32)
        )
      );
    },
    validateJwtPayload() {
      throw new Error(
        'Wasm JWT validation requires a datastore; use TestAuth.fromJwtPayload through a ModuleTestHarness.'
      );
    },
  };
}

class WasmContext implements NativeContext {
  readonly #ds: WasmPortableDatastore;

  constructor(ds: WasmPortableDatastore) {
    this.#ds = ds;
  }

  reset(): void {
    this.#ds.reset();
  }

  tableId(name: string): number {
    return this.#ds.tableId(name);
  }

  indexId(name: string): number {
    return this.#ds.indexId(name);
  }

  tableRowCount(target: NativeTarget, tableId: number): number {
    return isWasmTx(target)
      ? this.#ds.tableRowCountTx(target, tableId)
      : this.#ds.tableRowCount(tableId);
  }

  tableRows(target: NativeTarget, tableId: number): Uint8Array[] {
    return isWasmTx(target)
      ? this.#ds.tableRowsBsatnTx(target, tableId)
      : this.#ds.tableRowsBsatn(tableId);
  }

  insertBsatn(
    target: NativeTarget,
    tableId: number,
    row: Uint8Array
  ): Uint8Array {
    return this.#withMutTx(target, tx =>
      this.#ds.insertBsatnGeneratedCols(tx, tableId, row)
    );
  }

  deleteAllByEqBsatn(
    target: NativeTarget,
    tableId: number,
    relation: Uint8Array
  ): number {
    return this.#withMutTx(target, tx =>
      this.#ds.deleteByRelBsatn(tx, tableId, relation)
    );
  }

  indexScanPointBsatn(
    target: NativeTarget,
    indexId: number,
    point: Uint8Array
  ): Uint8Array[] {
    return isWasmTx(target)
      ? this.#ds.indexScanPointBsatnTx(target, indexId, point)
      : this.#ds.indexScanPointBsatn(indexId, point);
  }

  indexScanRangeBsatn(
    target: NativeTarget,
    indexId: number,
    buffer: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): Uint8Array[] {
    const { prefix, rstart, rend } = splitRangeBuffer(
      buffer,
      rstartLen,
      rendLen
    );
    return isWasmTx(target)
      ? this.#ds.indexScanRangeBsatnTx(
          target,
          indexId,
          prefix,
          prefixElems,
          rstart,
          rend
        )
      : this.#ds.indexScanRangeBsatn(
          indexId,
          prefix,
          prefixElems,
          rstart,
          rend
        );
  }

  deleteByIndexScanPointBsatn(
    target: NativeTarget,
    indexId: number,
    point: Uint8Array
  ): number {
    return this.#withMutTx(target, tx =>
      this.#ds.deleteByIndexScanPointBsatn(tx, indexId, point)
    );
  }

  deleteByIndexScanRangeBsatn(
    target: NativeTarget,
    indexId: number,
    buffer: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): number {
    const { prefix, rstart, rend } = splitRangeBuffer(
      buffer,
      rstartLen,
      rendLen
    );
    return this.#withMutTx(target, tx =>
      this.#ds.deleteByIndexScanRangeBsatn(
        tx,
        indexId,
        prefix,
        prefixElems,
        rstart,
        rend
      )
    );
  }

  updateBsatn(
    target: NativeTarget,
    tableId: number,
    indexId: number,
    row: Uint8Array
  ): Uint8Array {
    return this.#withMutTx(target, tx =>
      this.#ds.updateBsatnGeneratedCols(tx, tableId, indexId, row)
    );
  }

  clearTable(target: NativeTarget, tableId: number): number {
    return this.#withMutTx(target, tx => this.#ds.clearTable(tx, tableId));
  }

  runQuery(sql: string, databaseIdentity: bigint): Uint8Array[] {
    return this.#ds.runQuery(sql, bigintToFixedHex(databaseIdentity, 32));
  }

  validateJwtPayload(
    jwtPayload: string,
    connectionId: bigint
  ): WasmValidatedAuth {
    return this.#ds.validateJwtPayload(
      jwtPayload,
      bigintToFixedHex(connectionId, 16)
    );
  }

  beginTx(): NativeTx {
    return this.#ds.beginMutTx() as NativeTx;
  }

  commitTx(tx: NativeTx, mode: NativeCommitMode = 'Normal'): void {
    this.#ds.commitTx(expectWasmTx(tx), mode);
  }

  abortTx(tx: NativeTx): void {
    this.#ds.rollbackTx(expectWasmTx(tx));
  }

  #withMutTx<T>(
    target: NativeTarget,
    body: (tx: WasmPortableTransaction) => T
  ): T {
    if (isWasmTx(target)) {
      return body(target);
    }

    const tx = this.#ds.beginMutTx();
    try {
      const ret = body(tx);
      this.#ds.commitTx(tx, 'Normal');
      return ret;
    } catch (e) {
      this.#ds.rollbackTx(tx);
      throw e;
    }
  }
}

function isWasmTx(target: NativeTarget): target is WasmPortableTransaction {
  return !(target instanceof WasmContext);
}

function expectWasmTx(target: NativeTarget): WasmPortableTransaction {
  if (!isWasmTx(target)) {
    throw new Error('operation requires an active wasm datastore transaction');
  }
  return target;
}

function splitRangeBuffer(
  buffer: Uint8Array,
  rstartLen: number,
  rendLen: number
) {
  const prefixLen = buffer.byteLength - rstartLen - rendLen;
  if (prefixLen < 0) {
    throw new Error('invalid index range buffer lengths');
  }
  return {
    prefix: buffer.subarray(0, prefixLen),
    rstart: buffer.subarray(prefixLen, prefixLen + rstartLen),
    rend: buffer.subarray(
      prefixLen + rstartLen,
      prefixLen + rstartLen + rendLen
    ),
  };
}

function bigintToFixedHex(value: bigint, bytes: number): string {
  return value.toString(16).padStart(bytes * 2, '0');
}
