import { createRequire } from 'node:module';
import { defaultWasmTestRuntime } from './default_wasm_runtime';

export interface NativeTestRuntime {
  createContext(moduleDef: Uint8Array, moduleIdentity: bigint): NativeContext;
  validateJwtPayload(jwtPayload: string): bigint;
}

export type NativeCommitMode = 'Normal' | 'DropEventTableRows';

export interface NativeContext {
  reset(): void;
  tableId(name: string): number;
  indexId(name: string): number;
  tableRowCount(target: NativeTarget, tableId: number): number;
  tableRows(target: NativeTarget, tableId: number): Uint8Array[];
  insertBsatn(
    target: NativeTarget,
    tableId: number,
    row: Uint8Array
  ): Uint8Array;
  deleteAllByEqBsatn(
    target: NativeTarget,
    tableId: number,
    row: Uint8Array
  ): number;
  indexScanPointBsatn(
    target: NativeTarget,
    indexId: number,
    point: Uint8Array
  ): Uint8Array[];
  indexScanRangeBsatn(
    target: NativeTarget,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): Uint8Array[];
  deleteByIndexScanPointBsatn(
    target: NativeTarget,
    indexId: number,
    point: Uint8Array
  ): number;
  deleteByIndexScanRangeBsatn(
    target: NativeTarget,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): number;
  updateBsatn(
    target: NativeTarget,
    tableId: number,
    indexId: number,
    row: Uint8Array
  ): Uint8Array;
  clearTable(target: NativeTarget, tableId: number): number;
  runQuery(sql: string, databaseIdentity: bigint): Uint8Array[];
  validateJwtPayload(
    jwtPayload: string,
    connectionId: bigint
  ): { senderHex: string; connectionIdHex: string | undefined };
  beginTx(): NativeTx;
  commitTx(tx: NativeTx, mode?: NativeCommitMode): void;
  abortTx(tx: NativeTx): void;
}

export type NativeTarget = NativeContext | NativeTx;

export interface NativeTx {
  readonly __nativeTxBrand: unique symbol;
}

declare global {
  // The actual N-API package should install itself here, or this loader can be
  // replaced with a package import once the native package name is finalized.
  // eslint-disable-next-line no-var
  var __spacetimedbTestRuntime: NativeTestRuntime | undefined;
}

export function loadNativeTestRuntime(): NativeTestRuntime {
  const runtime = globalThis.__spacetimedbTestRuntime;
  if (runtime) return runtime;

  try {
    const require = createRequire(import.meta.url);
    return require('@clockworklabs/spacetimedb-test-runtime-node') as NativeTestRuntime;
  } catch (cause) {
    void cause;
    return defaultWasmTestRuntime;
  }
}
