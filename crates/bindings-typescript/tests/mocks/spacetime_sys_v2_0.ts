export const moduleHooks = Symbol('spacetimedb.test.moduleHooks');

const tableIds = new Map<string, number>();
const indexIds = new Map<string, number>();

function nextId(map: Map<string, number>, name: string) {
  const existing = map.get(name);
  if (existing != null) return existing;
  const next = map.size + 1;
  map.set(name, next);
  return next;
}

export function __resetMockSys() {
  tableIds.clear();
  indexIds.clear();
}

export function register_hooks(_hooks: unknown) {}
export function table_id_from_name(name: string) {
  return nextId(tableIds, name);
}
export function index_id_from_name(name: string) {
  return nextId(indexIds, name);
}
export function datastore_table_row_count(_tableId: number) {
  return 0n;
}
export function datastore_table_scan_bsatn(_tableId: number) {
  return 0;
}
export function datastore_index_scan_range_bsatn() {
  return 0;
}
export function row_iter_bsatn_advance() {
  return 0;
}
export function row_iter_bsatn_close() {}
export function datastore_insert_bsatn() {
  return 0;
}
export function datastore_update_bsatn() {
  return 0;
}
export function datastore_delete_by_index_scan_range_bsatn() {
  return 0;
}
export function datastore_delete_all_by_eq_bsatn() {
  return 0;
}
export function volatile_nonatomic_schedule_immediate() {}
export function console_log() {}
export function console_timer_start() {
  return 0;
}
export function console_timer_end() {}
export function identity() {
  return 0n;
}
export function get_jwt_payload() {
  return new Uint8Array();
}
export function procedure_http_request() {
  return [new Uint8Array(), new Uint8Array()] as const;
}
export function procedure_start_mut_tx() {
  return 0n;
}
export function procedure_commit_mut_tx() {}
export function procedure_abort_mut_tx() {}
export function datastore_index_scan_point_bsatn() {
  return 0;
}
export function datastore_delete_by_index_scan_point_bsatn() {
  return 0;
}
