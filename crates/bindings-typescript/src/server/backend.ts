import * as _syscalls2_0 from 'spacetime:sys@2.0';
import * as _syscalls2_1 from 'spacetime:sys@2.1';

import type { u128, u16, u256, u32 } from 'spacetime:sys@2.0';

export const sys = { ..._syscalls2_0, ..._syscalls2_1 };

export interface DatastoreBackend {
  identity(): u256;
  getJwtPayload(connectionId: u128): Uint8Array;

  tableIdFromName(name: string): u32;
  indexIdFromName(name: string): u32;
  datastoreTableRowCount(tableId: u32): u64ish;
  datastoreTableScanBsatn(tableId: u32): u32;
  datastoreInsertBsatn(
    tableId: u32,
    row: ArrayBuffer,
    rowLen: number
  ): Uint8Array | number | void;
  datastoreDeleteAllByEqBsatn(
    tableId: u32,
    row: ArrayBuffer,
    rowLen: number
  ): u32;
  datastoreIndexScanPointBsatn(
    indexId: u32,
    point: ArrayBuffer,
    pointLen: number
  ): u32;
  datastoreIndexScanRangeBsatn(
    indexId: u32,
    prefix: ArrayBuffer,
    prefixLen: u32,
    prefixElems: u16,
    rstartLen: u32,
    rendLen: u32
  ): u32;
  datastoreDeleteByIndexScanPointBsatn(
    indexId: u32,
    point: ArrayBuffer,
    pointLen: number
  ): u32;
  datastoreDeleteByIndexScanRangeBsatn(
    indexId: u32,
    prefix: ArrayBuffer,
    prefixLen: u32,
    prefixElems: u16,
    rstartLen: u32,
    rendLen: u32
  ): u32;
  datastoreUpdateBsatn(
    tableId: u32,
    indexId: u32,
    row: ArrayBuffer,
    rowLen: number
  ): Uint8Array | number | void;
  datastoreClear(tableId: u32): void;

  rowIterBsatnAdvance(iterId: u32, out: ArrayBuffer): number;
  rowIterBsatnClose(iterId: u32): void;

  procedureStartMutTx(): bigint;
  procedureCommitMutTx(): void;
  procedureAbortMutTx(): void;
  procedureHttpRequest(
    request: Uint8Array,
    body: Uint8Array | string
  ): [Uint8Array, Uint8Array];
}

type u64ish = number | bigint;

export const hostBackend: DatastoreBackend = {
  // Direct references keep backend injection from adding a wrapper call to the host hot path.
  identity: sys.identity,
  getJwtPayload: sys.get_jwt_payload,
  tableIdFromName: sys.table_id_from_name,
  indexIdFromName: sys.index_id_from_name,
  datastoreTableRowCount: sys.datastore_table_row_count,
  datastoreTableScanBsatn: sys.datastore_table_scan_bsatn,
  datastoreInsertBsatn: sys.datastore_insert_bsatn,
  datastoreDeleteAllByEqBsatn: sys.datastore_delete_all_by_eq_bsatn,
  datastoreIndexScanPointBsatn: sys.datastore_index_scan_point_bsatn,
  datastoreIndexScanRangeBsatn: sys.datastore_index_scan_range_bsatn,
  datastoreDeleteByIndexScanPointBsatn:
    sys.datastore_delete_by_index_scan_point_bsatn,
  datastoreDeleteByIndexScanRangeBsatn:
    sys.datastore_delete_by_index_scan_range_bsatn,
  datastoreUpdateBsatn: sys.datastore_update_bsatn,
  datastoreClear: sys.datastore_clear,
  rowIterBsatnAdvance: sys.row_iter_bsatn_advance,
  rowIterBsatnClose: sys.row_iter_bsatn_close,
  procedureStartMutTx: sys.procedure_start_mut_tx,
  procedureCommitMutTx: sys.procedure_commit_mut_tx,
  procedureAbortMutTx: sys.procedure_abort_mut_tx,
  procedureHttpRequest: sys.procedure_http_request,
};
