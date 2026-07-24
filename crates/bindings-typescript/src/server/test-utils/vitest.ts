import type { Plugin } from 'vite';

export function spacetimedbModuleTestPlugin(): Plugin {
  return {
    name: 'spacetimedb-module-test',
    enforce: 'pre',
    resolveId(id) {
      if (id === 'spacetime:sys@2.0' || id === 'spacetime:sys@2.1') {
        return '\0spacetimedb-module-test-sys';
      }
      return null;
    },
    load(id) {
      if (id !== '\0spacetimedb-module-test-sys') return null;
      return `
        export const moduleHooks = Symbol.for('spacetimedb.moduleHooks');
        const unsupported = name => () => {
          throw new Error('Unsupported SpacetimeDB host syscall in module unit test: ' + name);
        };
        export const identity = unsupported('identity');
        export const get_jwt_payload = unsupported('get_jwt_payload');
        export const table_id_from_name = unsupported('table_id_from_name');
        export const index_id_from_name = unsupported('index_id_from_name');
        export const datastore_table_row_count = unsupported('datastore_table_row_count');
        export const datastore_table_scan_bsatn = unsupported('datastore_table_scan_bsatn');
        export const datastore_insert_bsatn = unsupported('datastore_insert_bsatn');
        export const datastore_delete_all_by_eq_bsatn = unsupported('datastore_delete_all_by_eq_bsatn');
        export const datastore_index_scan_point_bsatn = unsupported('datastore_index_scan_point_bsatn');
        export const datastore_index_scan_range_bsatn = unsupported('datastore_index_scan_range_bsatn');
        export const datastore_delete_by_index_scan_point_bsatn = unsupported('datastore_delete_by_index_scan_point_bsatn');
        export const datastore_delete_by_index_scan_range_bsatn = unsupported('datastore_delete_by_index_scan_range_bsatn');
        export const datastore_update_bsatn = unsupported('datastore_update_bsatn');
        export const datastore_clear = unsupported('datastore_clear');
        export const row_iter_bsatn_advance = unsupported('row_iter_bsatn_advance');
        export const row_iter_bsatn_close = unsupported('row_iter_bsatn_close');
        export const procedure_start_mut_tx = unsupported('procedure_start_mut_tx');
        export const procedure_commit_mut_tx = unsupported('procedure_commit_mut_tx');
        export const procedure_abort_mut_tx = unsupported('procedure_abort_mut_tx');
        export const procedure_http_request = unsupported('procedure_http_request');
        export const console_log = () => {};
        export const console_timer_start = () => 0;
        export const console_timer_end = () => {};
        export const volatile_nonatomic_schedule_immediate = unsupported('volatile_nonatomic_schedule_immediate');
      `;
    },
  };
}
