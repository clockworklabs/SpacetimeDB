import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import type {
  UseQueryOptions,
  UseQueryResult,
  UseSuspenseQueryOptions,
  UseSuspenseQueryResult,
} from '@tanstack/react-query';
import type { UntypedTableDef, RowType } from '../lib/table';
import type { Expr, ColumnsFromRow } from '../lib/filter';
import { spacetimeDBQuery } from './SpacetimeDBQueryClient';

export type UseSpacetimeDBQueryResult<T> = [
  T[],
  boolean,
  UseQueryResult<T[], Error>,
];

export type UseSpacetimeDBSuspenseQueryResult<T> = [
  T[],
  false,
  UseSuspenseQueryResult<T[], Error>,
];

// Wraps TanStack Query useQuery and returns [data, loading, query]
// pass 'skip' as the filter to set enabled: false, disabling the query
// until a condition is met
export function useSpacetimeDBQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  whereOrSkip?: Expr<ColumnsFromRow<RowType<TableDef>>> | 'skip',
  // any useQuery option (e.g. enabled, refetchInterval, select, placeholderData),
  // except queryKey, queryFn, and meta (managed internally)
  options?: Omit<
    UseQueryOptions<
      RowType<TableDef>[],
      Error,
      RowType<TableDef>[],
      readonly ['spacetimedb', string, string]
    >,
    'queryKey' | 'queryFn' | 'meta'
  >
): UseSpacetimeDBQueryResult<RowType<TableDef>> {
  const queryOptions =
    whereOrSkip === 'skip'
      ? spacetimeDBQuery(table, 'skip')
      : spacetimeDBQuery(table, whereOrSkip);

  const query = useQuery({
    ...queryOptions,
    ...options,
  } as UseQueryOptions<RowType<TableDef>[], Error>);

  return [query.data ?? [], query.isPending, query];
}

// Suspense version of useSpacetimeDBQuery, returns [data, false, query] tuple (loading = false)
// Instead of returning a loading boolean, this hook suspends the component
// until data is ready, a parent <Suspense fallback={…}> handles the loading UI.
// does not support 'skip' because useSuspenseQuery must always resolve
export function useSpacetimeDBSuspenseQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  where?: Expr<ColumnsFromRow<RowType<TableDef>>>,
  options?: Omit<
    UseSuspenseQueryOptions<
      RowType<TableDef>[],
      Error,
      RowType<TableDef>[],
      readonly ['spacetimedb', string, string]
    >,
    'queryKey' | 'queryFn' | 'meta'
  >
): UseSpacetimeDBSuspenseQueryResult<RowType<TableDef>> {
  const queryOptions = spacetimeDBQuery(table, where);

  const query = useSuspenseQuery({
    ...queryOptions,
    ...options,
  } as UseSuspenseQueryOptions<RowType<TableDef>[], Error>);

  return [query.data, false, query];
}
