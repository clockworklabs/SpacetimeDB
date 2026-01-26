import { useQuery, useSuspenseQuery } from '@tanstack/react-query';
import type {
  UseQueryOptions,
  UseQueryResult,
  UseSuspenseQueryOptions,
  UseSuspenseQueryResult,
} from '@tanstack/react-query';
import type { UntypedTableDef, RowType } from '../lib/table';
import type { Expr, ColumnsFromRow } from '../react/useTable';
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

// returns [data, loading, query] tuple
export function useSpacetimeDBQuery<TableDef extends UntypedTableDef>(
  table: TableDef,
  whereOrSkip?: Expr<ColumnsFromRow<RowType<TableDef>>> | 'skip',
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

// returns [data, false, query] tuple
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
