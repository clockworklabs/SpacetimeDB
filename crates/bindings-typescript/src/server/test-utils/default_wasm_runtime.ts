import { createRequire } from 'node:module';
import {
  createWasmTestRuntime,
  type WasmCommitMode,
  type WasmPortableDatastore,
  type WasmPortableDatastoreModule,
  type WasmPortableTransaction,
  type WasmValidatedAuth,
} from './wasm';
import type {
  WasmPortableDatastore as GeneratedPortableDatastore,
  WasmPortableTransaction as GeneratedPortableTransaction,
} from './portable-datastore-wasm/spacetimedb_portable_datastore_wasm';

const require = createRequire(import.meta.url);
const generated = require('./portable-datastore-wasm/spacetimedb_portable_datastore_wasm.cjs') as {
  WasmCommitMode: {
    Normal: number;
    DropEventTableRows: number;
  };
  WasmPortableDatastore: new (
    rawModuleDefBsatn: Uint8Array,
    moduleIdentityHex: string
  ) => GeneratedPortableDatastore;
};

class DefaultWasmPortableDatastore implements WasmPortableDatastore {
  readonly #inner: GeneratedPortableDatastore;

  constructor(rawModuleDefBsatn: Uint8Array, moduleIdentityHex: string) {
    this.#inner = new generated.WasmPortableDatastore(
      rawModuleDefBsatn,
      moduleIdentityHex
    );
  }

  tableId(name: string): number {
    return this.#inner.tableId(name);
  }

  indexId(name: string): number {
    return this.#inner.indexId(name);
  }

  beginMutTx(): WasmPortableTransaction {
    return this.#inner.beginMutTx() as unknown as WasmPortableTransaction;
  }

  commitTx(tx: WasmPortableTransaction, mode: WasmCommitMode): void {
    this.#inner.commitTx(
      tx as unknown as GeneratedPortableTransaction,
      mode === 'DropEventTableRows'
        ? generated.WasmCommitMode.DropEventTableRows
        : generated.WasmCommitMode.Normal
    );
  }

  rollbackTx(tx: WasmPortableTransaction): void {
    this.#inner.rollbackTx(tx as unknown as GeneratedPortableTransaction);
  }

  reset(): void {
    this.#inner.reset();
  }

  tableRowCount(tableId: number): number {
    return this.#inner.tableRowCount(tableId);
  }

  tableRowCountTx(tx: WasmPortableTransaction, tableId: number): number {
    return this.#inner.tableRowCountTx(
      tx as unknown as GeneratedPortableTransaction,
      tableId
    );
  }

  tableRowsBsatn(tableId: number): Uint8Array[] {
    return arrayFromWasmRows(this.#inner.tableRowsBsatn(tableId));
  }

  tableRowsBsatnTx(tx: WasmPortableTransaction, tableId: number): Uint8Array[] {
    return arrayFromWasmRows(
      this.#inner.tableRowsBsatnTx(
        tx as unknown as GeneratedPortableTransaction,
        tableId
      )
    );
  }

  indexScanPointBsatn(indexId: number, point: Uint8Array): Uint8Array[] {
    return arrayFromWasmRows(this.#inner.indexScanPointBsatn(indexId, point));
  }

  indexScanPointBsatnTx(
    tx: WasmPortableTransaction,
    indexId: number,
    point: Uint8Array
  ): Uint8Array[] {
    return arrayFromWasmRows(
      this.#inner.indexScanPointBsatnTx(
        tx as unknown as GeneratedPortableTransaction,
        indexId,
        point
      )
    );
  }

  indexScanRangeBsatn(
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): Uint8Array[] {
    return arrayFromWasmRows(
      this.#inner.indexScanRangeBsatn(
        indexId,
        prefix,
        prefixElems,
        rstart,
        rend
      )
    );
  }

  indexScanRangeBsatnTx(
    tx: WasmPortableTransaction,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): Uint8Array[] {
    return arrayFromWasmRows(
      this.#inner.indexScanRangeBsatnTx(
        tx as unknown as GeneratedPortableTransaction,
        indexId,
        prefix,
        prefixElems,
        rstart,
        rend
      )
    );
  }

  insertBsatnGeneratedCols(
    tx: WasmPortableTransaction,
    tableId: number,
    row: Uint8Array
  ): Uint8Array {
    return this.#inner.insertBsatnGeneratedCols(
      tx as unknown as GeneratedPortableTransaction,
      tableId,
      row
    );
  }

  updateBsatnGeneratedCols(
    tx: WasmPortableTransaction,
    tableId: number,
    indexId: number,
    row: Uint8Array
  ): Uint8Array {
    return this.#inner.updateBsatnGeneratedCols(
      tx as unknown as GeneratedPortableTransaction,
      tableId,
      indexId,
      row
    );
  }

  deleteByRelBsatn(
    tx: WasmPortableTransaction,
    tableId: number,
    relation: Uint8Array
  ): number {
    return this.#inner.deleteByRelBsatn(
      tx as unknown as GeneratedPortableTransaction,
      tableId,
      relation
    );
  }

  deleteByIndexScanPointBsatn(
    tx: WasmPortableTransaction,
    indexId: number,
    point: Uint8Array
  ): number {
    return this.#inner.deleteByIndexScanPointBsatn(
      tx as unknown as GeneratedPortableTransaction,
      indexId,
      point
    );
  }

  deleteByIndexScanRangeBsatn(
    tx: WasmPortableTransaction,
    indexId: number,
    prefix: Uint8Array,
    prefixElems: number,
    rstart: Uint8Array,
    rend: Uint8Array
  ): number {
    return this.#inner.deleteByIndexScanRangeBsatn(
      tx as unknown as GeneratedPortableTransaction,
      indexId,
      prefix,
      prefixElems,
      rstart,
      rend
    );
  }

  clearTable(tx: WasmPortableTransaction, tableId: number): number {
    return this.#inner.clearTable(
      tx as unknown as GeneratedPortableTransaction,
      tableId
    );
  }

  validateJwtPayload(
    payload: string,
    connectionIdHex: string
  ): WasmValidatedAuth {
    return this.#inner.validateJwtPayload(payload, connectionIdHex);
  }
}

function arrayFromWasmRows(rows: Array<unknown>): Uint8Array[] {
  return rows.map(row => row as Uint8Array);
}

export const defaultWasmTestRuntime = createWasmTestRuntime({
  WasmPortableDatastore:
    DefaultWasmPortableDatastore as WasmPortableDatastoreModule['WasmPortableDatastore'],
});
