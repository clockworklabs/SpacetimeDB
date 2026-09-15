import { useMemo } from 'react';
import { useTable, type UseTableCallbacks } from './useTable';
import type { UntypedTableDef, RowType } from '../lib/table';
import type { Prettify } from '../lib/type_util';
import type { Query } from '../lib/query';

/**
 * React hook to subscribe to a view or table in SpacetimeDB and receive a single live row.
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
): [Prettify<RowType<TableDef>> | undefined, boolean] {
  const [rows, isReady] = useTable(query, callbacks);
  const row = useMemo(() => rows[0] ?? undefined, [rows]);
  return [row, isReady];
}
