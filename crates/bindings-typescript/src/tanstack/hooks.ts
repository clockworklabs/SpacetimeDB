import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import type {
  UseQueryOptions,
  UseQueryResult,
  UseSuspenseQueryOptions,
  UseSuspenseQueryResult,
} from '@tanstack/react-query';
import type { UntypedTableDef, RowType } from '../lib/table';
import type { Query } from '../lib/query';
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
// pass 'skip' as the second argument to set enabled: false, disabling the query
// until a condition is met
//
// Usage:
//   useSpacetimeDBQuery(tables.person)
//   useSpacetimeDBQuery(tables.user.where(r => r.online.eq(true)))
//   useSpacetimeDBQuery(condition ? tables.user : 'skip')
export function useSpacetimeDBQuery<TableDef extends UntypedTableDef>(
  queryOrSkip: Query<TableDef> | 'skip',
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
    queryOrSkip === 'skip'
      ? spacetimeDBQuery('skip')
      : spacetimeDBQuery(queryOrSkip);

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
  query: Query<TableDef>,
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
  const queryOptions = spacetimeDBQuery(query);

  const q = useSuspenseQuery({
    ...queryOptions,
    ...options,
  } as UseSuspenseQueryOptions<RowType<TableDef>[], Error>);

  return [q.data, false, q];
}
