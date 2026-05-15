import { defaultWasmTestRuntime } from './default_wasm_runtime';

export interface TestRuntime {
  createContext(moduleDef: Uint8Array, moduleIdentity: bigint): TestRuntimeContext;
}

export type TestRuntimeCommitMode = 'Normal' | 'DropEventTableRows';

export interface TestRuntimeContext {
  reset(): void;
  tableId(name: string): number;
  indexId(name: string): number;
  tableRowCount(target: TestRuntimeTarget, tableId: number): number;
  tableRows(target: TestRuntimeTarget, tableId: number): Uint8Array[];
  insertBsatn(
    target: TestRuntimeTarget,
    tableId: number,
    row: Uint8Array
  ): Uint8Array;
  deleteAllByEqBsatn(
    target: TestRuntimeTarget,
    tableId: number,
    row: Uint8Array
  ): number;
  indexScanPointBsatn(
    target: TestRuntimeTarget,
    indexId: number,
    point: Uint8Array
  ): Uint8Array[];
  indexScanRangeBsatn(
    target: TestRuntimeTarget,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): Uint8Array[];
  deleteByIndexScanPointBsatn(
    target: TestRuntimeTarget,
    indexId: number,
    point: Uint8Array
  ): number;
  deleteByIndexScanRangeBsatn(
    target: TestRuntimeTarget,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstartLen: number,
    rendLen: number
  ): number;
  updateBsatn(
    target: TestRuntimeTarget,
    tableId: number,
    indexId: number,
    row: Uint8Array
  ): Uint8Array;
  clearTable(target: TestRuntimeTarget, tableId: number): number;
  runQuery(sql: string, databaseIdentity: bigint): Uint8Array[];
  validateJwtPayload(
    jwtPayload: string,
    connectionId: bigint
  ): { senderHex: string; connectionIdHex: string | undefined };
  beginTx(): TestRuntimeTx;
  commitTx(tx: TestRuntimeTx, mode?: TestRuntimeCommitMode): void;
  abortTx(tx: TestRuntimeTx): void;
}

export type TestRuntimeTarget = TestRuntimeContext | TestRuntimeTx;

export interface TestRuntimeTx {
  readonly __testRuntimeTxBrand: unique symbol;
}

declare global {
  // Tests may override the runtime to exercise the adapter boundary.
  // eslint-disable-next-line no-var
  var __spacetimedbTestRuntime: TestRuntime | undefined;
}

export function loadTestRuntime(): TestRuntime {
  return globalThis.__spacetimedbTestRuntime ?? defaultWasmTestRuntime;
}
