import type { RowType, UntypedTableDef } from './table';
import { Uuid } from './uuid';

export type Value = string | number | boolean | Uuid;

export type Expr<Column extends string> =
  | { type: 'eq'; key: Column; value: Value }
  | { type: 'and'; children: Expr<Column>[] }
  | { type: 'or'; children: Expr<Column>[] };

export const eq = <Column extends string>(
  key: Column,
  value: Value
): Expr<Column> => ({ type: 'eq', key, value });

export const and = <Column extends string>(
  ...children: Expr<Column>[]
): Expr<Column> => {
  const flat: Expr<Column>[] = [];
  for (const c of children) {
    if (!c) continue;
    if (c.type === 'and') flat.push(...c.children);
    else flat.push(c);
  }
  const pruned = flat.filter(Boolean);
  if (pruned.length === 0) return { type: 'and', children: [] };
  if (pruned.length === 1) return pruned[0];
  return { type: 'and', children: pruned };
};

export const or = <Column extends string>(
  ...children: Expr<Column>[]
): Expr<Column> => {
  const flat: Expr<Column>[] = [];
  for (const c of children) {
    if (!c) continue;
    if (c.type === 'or') flat.push(...c.children);
    else flat.push(c);
  }
  const pruned = flat.filter(Boolean);
  if (pruned.length === 0) return { type: 'or', children: [] };
  if (pruned.length === 1) return pruned[0];
  return { type: 'or', children: pruned };
};

export const isEq = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'eq' }> => e.type === 'eq';
export const isAnd = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'and' }> => e.type === 'and';
export const isOr = <Column extends string>(
  e: Expr<Column>
): e is Extract<Expr<Column>, { type: 'or' }> => e.type === 'or';

export function evaluate<Column extends string>(
  expr: Expr<Column>,
  row: Record<Column, any>
): boolean {
  switch (expr.type) {
    case 'eq': {
      // The actual value of the Column
      const v = row[expr.key];
      if (
        typeof v === 'string' ||
        typeof v === 'number' ||
        typeof v === 'boolean'
      ) {
        return v === expr.value;
      }
      if (typeof v === 'object') {
        // Value of the Column and passed Value are both a Uuid so do an integer comparison.
        if (v instanceof Uuid && expr.value instanceof Uuid) {
          return v.asBigInt() === expr.value.asBigInt();
        }
        // Value of the Column is a Uuid but passed Value is a String so compare them via string.
        if (v instanceof Uuid && typeof expr.value === 'string') {
          return v.toString() === expr.value;
        }
      }
      return false;
    }
    case 'and':
      return (
        expr.children.length === 0 || expr.children.every(c => evaluate(c, row))
      );
    case 'or':
      return (
        expr.children.length !== 0 && expr.children.some(c => evaluate(c, row))
      );
  }
}

function formatValue(v: Value): string {
  switch (typeof v) {
    case 'string':
      return `'${v.replace(/'/g, "''")}'`;
    case 'number':
      return Number.isFinite(v) ? String(v) : `'${String(v)}'`;
    case 'boolean':
      return v ? 'TRUE' : 'FALSE';
    case 'object': {
      if (v instanceof Uuid) {
        return `'${v.toString()}'`;
      }

      return '';
    }
  }
}

function escapeIdent(id: string): string {
  if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(id)) return id;
  return `"${id.replace(/"/g, '""')}"`;
}

function parenthesize(s: string): string {
  if (!s.includes(' AND ') && !s.includes(' OR ')) return s;
  return `(${s})`;
}

export function toString<TableDef extends UntypedTableDef>(
  tableDef: TableDef,
  expr: Expr<ColumnsFromRow<RowType<TableDef>>>
): string {
  switch (expr.type) {
    case 'eq': {
      const key = tableDef.columns[expr.key].columnMetadata.name ?? expr.key;
      return `${escapeIdent(key)} = ${formatValue(expr.value)}`;
    }
    case 'and':
      return parenthesize(
        expr.children.map(expr => toString(tableDef, expr)).join(' AND ')
      );
    case 'or':
      return parenthesize(
        expr.children.map(expr => toString(tableDef, expr)).join(' OR ')
      );
  }
}

/**
 * This is just the identity function to make things look like SQL.
 * @param expr
 * @returns
 */
export function where<Column extends string>(expr: Expr<Column>): Expr<Column> {
  return expr;
}

type MembershipChange = 'enter' | 'leave' | 'stayIn' | 'stayOut';

export function classifyMembership<
  Col extends string,
  R extends Record<string, unknown>,
>(where: Expr<Col> | undefined, oldRow: R, newRow: R): MembershipChange {
  // No filter: everything is in, so updates are always "stayIn".
  if (!where) {
    return 'stayIn';
  }

  const oldIn = evaluate(where, oldRow);
  const newIn = evaluate(where, newRow);

  if (oldIn && !newIn) {
    return 'leave';
  }
  if (!oldIn && newIn) {
    return 'enter';
  }
  if (oldIn && newIn) {
    return 'stayIn';
  }
  return 'stayOut';
}

/**
 * Extracts the column names from a RowType whose values are of type Value.
 * Note that this will exclude columns that are of type object, array, etc.
 */
export type ColumnsFromRow<R> = {
  [K in keyof R]-?: R[K] extends Value | undefined ? K : never;
}[keyof R] &
  string;
