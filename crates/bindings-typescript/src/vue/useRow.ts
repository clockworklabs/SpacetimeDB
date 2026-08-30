import { computed, type Ref, type DeepReadonly } from 'vue';
import { useTable, type UseTableCallbacks } from './useTable';
import type { UntypedTableDef, RowType } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import type { Query } from '../lib/query';

/**
 * Vue composable to subscribe to a view or table in SpacetimeDB and receive a single live row.
 *
 * This is particularly useful for views that return `t.option(Row)`.
 *
 * @param query - A query builder expression (table reference or filtered query).
 * @param callbacks - Optional callbacks for row insert, delete, and update events.
 * @returns A tuple of [row, isReady], where row is `undefined` if not found.
 */
export function useRow<TableDef extends UntypedTableDef>(
  query: Query<TableDef>,
  callbacks?: UseTableCallbacks<Prettify<RowType<TableDef>>>
): [
  DeepReadonly<Ref<Prettify<RowType<TableDef>> | undefined>>,
  DeepReadonly<Ref<boolean>>,
] {
  const [rows, isReady] = useTable(query, callbacks);
  const row = computed(() => rows.value[0] ?? undefined);
  return [row, isReady];
}
