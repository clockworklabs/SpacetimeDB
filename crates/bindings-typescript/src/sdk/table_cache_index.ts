import { AlgebraicType, type ComparablePrimitive } from '../lib/algebraic_type';
import type { UntypedTableDef } from '../lib/table';

type IndexNode = {
  children: Map<ComparablePrimitive, IndexNode>;
  rows: Set<ComparablePrimitive>;
};

/**
 * Dictionary lookups for full keys and equality prefixes of a cache index.
 * Each level represents one column, so tuple keys cannot collide through string
 * concatenation. Lookup takes O(key columns), plus O(matches) to return rows.
 */
export class TableCacheIndex {
  readonly #roots = new Map<ComparablePrimitive, IndexNode>();
  readonly #keyTypes: AlgebraicType[];

  constructor(
    tableDef: UntypedTableDef,
    readonly columns: readonly string[]
  ) {
    this.#keyTypes = columns.map(
      column => tableDef.columns[column].typeBuilder.algebraicType
    );
  }

  #rowKey(row: Record<string, any>): ComparablePrimitive[] {
    return this.columns.map((column, i) =>
      AlgebraicType.intoMapKey(this.#keyTypes[i], row[column])
    );
  }

  /** Store row IDs so reference-count changes always expose the current row. */
  replace(
    rowId: ComparablePrimitive,
    oldRow: Record<string, any> | undefined,
    newRow: Record<string, any>
  ): void {
    const key = this.#rowKey(newRow);
    if (oldRow) {
      const oldKey = this.#rowKey(oldRow);
      if (key.every((value, i) => value === oldKey[i])) return;
      this.#remove(rowId, oldKey);
    }
    let children = this.#roots;
    for (const value of key) {
      let node = children.get(value);
      if (!node) {
        node = { children: new Map(), rows: new Set() };
        children.set(value, node);
      }
      node.rows.add(rowId);
      children = node.children;
    }
  }

  remove(rowId: ComparablePrimitive, row: Record<string, any>): void {
    this.#remove(rowId, this.#rowKey(row));
  }

  #remove(
    rowId: ComparablePrimitive,
    key: readonly ComparablePrimitive[]
  ): void {
    let children = this.#roots;
    for (const value of key) {
      const node = children.get(value);
      if (!node) return;
      node.rows.delete(rowId);
      if (node.rows.size === 0) {
        children.delete(value);
      }
      // Clear every level, even if its parent was pruned: a filter iterator may
      // still be consuming one of the descendant sets.
      children = node.children;
    }
  }

  lookup(
    values: readonly unknown[]
  ): ReadonlySet<ComparablePrimitive> | undefined {
    if (values.length === 0 || values.length > this.columns.length) return;
    let children = this.#roots;
    let node: IndexNode | undefined;
    for (let i = 0; i < values.length; i++) {
      // Normalize wrapper and structured values by value, not object identity.
      const key = AlgebraicType.intoMapKey(this.#keyTypes[i], values[i]);
      node = children.get(key);
      if (!node) return;
      children = node.children;
    }
    return node?.rows;
  }
}
