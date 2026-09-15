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
  identity: () => sys.identity(),
  getJwtPayload: connectionId => sys.get_jwt_payload(connectionId),
  tableIdFromName: name => sys.table_id_from_name(name),
  indexIdFromName: name => sys.index_id_from_name(name),
  datastoreTableRowCount: tableId => sys.datastore_table_row_count(tableId),
  datastoreTableScanBsatn: tableId => sys.datastore_table_scan_bsatn(tableId),
  datastoreInsertBsatn: (tableId, row, rowLen) =>
    sys.datastore_insert_bsatn(tableId, row, rowLen),
  datastoreDeleteAllByEqBsatn: (tableId, row, rowLen) =>
    sys.datastore_delete_all_by_eq_bsatn(tableId, row, rowLen),
  datastoreIndexScanPointBsatn: (indexId, point, pointLen) =>
    sys.datastore_index_scan_point_bsatn(indexId, point, pointLen),
  datastoreIndexScanRangeBsatn: (
    indexId,
    prefix,
    prefixLen,
    prefixElems,
    rstartLen,
    rendLen
  ) =>
    sys.datastore_index_scan_range_bsatn(
      indexId,
      prefix,
      prefixLen,
      prefixElems,
      rstartLen,
      rendLen
    ),
  datastoreDeleteByIndexScanPointBsatn: (indexId, point, pointLen) =>
    sys.datastore_delete_by_index_scan_point_bsatn(indexId, point, pointLen),
  datastoreDeleteByIndexScanRangeBsatn: (
    indexId,
    prefix,
    prefixLen,
    prefixElems,
    rstartLen,
    rendLen
  ) =>
    sys.datastore_delete_by_index_scan_range_bsatn(
      indexId,
      prefix,
      prefixLen,
      prefixElems,
      rstartLen,
      rendLen
    ),
  datastoreUpdateBsatn: (tableId, indexId, row, rowLen) =>
    sys.datastore_update_bsatn(tableId, indexId, row, rowLen),
  datastoreClear: tableId => sys.datastore_clear(tableId),
  rowIterBsatnAdvance: (iterId, out) => sys.row_iter_bsatn_advance(iterId, out),
  rowIterBsatnClose: iterId => sys.row_iter_bsatn_close(iterId),
  procedureStartMutTx: () => sys.procedure_start_mut_tx(),
  procedureCommitMutTx: () => sys.procedure_commit_mut_tx(),
  procedureAbortMutTx: () => sys.procedure_abort_mut_tx(),
  procedureHttpRequest: (request, body) =>
    sys.procedure_http_request(request, body),
};
