/* tslint:disable */
/* eslint-disable */
export enum WasmCommitMode {
  Normal = 0,
  DropEventTableRows = 1,
}
export class WasmPortableDatastore {
  free(): void;
  [Symbol.dispose](): void;
  clearTable(tx: WasmPortableTransaction, table_id: number): number;
  rollbackTx(tx: WasmPortableTransaction): void;
  beginMutTx(): WasmPortableTransaction;
  tableRowCount(table_id: number): number;
  tableRowsBsatn(table_id: number): Array<any>;
  tableRowCountTx(tx: WasmPortableTransaction, table_id: number): number;
  deleteByRelBsatn(tx: WasmPortableTransaction, table_id: number, relation: Uint8Array): number;
  tableRowsBsatnTx(tx: WasmPortableTransaction, table_id: number): Array<any>;
  validateJwtPayload(payload: string, connection_id_hex: string): WasmValidatedAuth;
  indexScanPointBsatn(index_id: number, point: Uint8Array): Array<any>;
  indexScanRangeBsatn(index_id: number, prefix: Uint8Array, prefix_elems: number, rstart: Uint8Array, rend: Uint8Array): Array<any>;
  indexScanPointBsatnTx(tx: WasmPortableTransaction, index_id: number, point: Uint8Array): Array<any>;
  indexScanRangeBsatnTx(tx: WasmPortableTransaction, index_id: number, prefix: Uint8Array, prefix_elems: number, rstart: Uint8Array, rend: Uint8Array): Array<any>;
  insertBsatnGeneratedCols(tx: WasmPortableTransaction, table_id: number, row: Uint8Array): Uint8Array;
  updateBsatnGeneratedCols(tx: WasmPortableTransaction, table_id: number, index_id: number, row: Uint8Array): Uint8Array;
  deleteByIndexScanPointBsatn(tx: WasmPortableTransaction, index_id: number, point: Uint8Array): number;
  deleteByIndexScanRangeBsatn(tx: WasmPortableTransaction, index_id: number, prefix: Uint8Array, prefix_elems: number, rstart: Uint8Array, rend: Uint8Array): number;
  constructor(raw_module_def_bsatn: Uint8Array, module_identity_hex: string);
  reset(): void;
  indexId(index_name: string): number;
  tableId(table_name: string): number;
  commitTx(tx: WasmPortableTransaction, mode: WasmCommitMode): void;
  runQuery(sql: string, database_identity_hex: string): Array<any>;
}
export class WasmPortableTransaction {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
}
export class WasmValidatedAuth {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  readonly senderHex: string;
  readonly connectionIdHex: string | undefined;
}
