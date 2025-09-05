import { describe, it, expect } from 'vitest';
import { AlgebraicType, t } from '../src/index';

describe('TypeBuilder', () => {
  it('builds the correct algebraic type for a point', () => {
    const point = t.object({
      x: t.f64(),
      y: t.f64(),
      z: t.f64(),
    });
    expect(point.algebraicType).toEqual({
      tag: 'Product',
      value: {
        elements: [
          { name: 'x', algebraicType: AlgebraicType.F64 },
          { name: 'y', algebraicType: AlgebraicType.F64 },
          { name: 'z', algebraicType: AlgebraicType.F64 },
        ],
      },
    });
  });

  it('builds the correct algebraic type for a sum type', () => {
    const sumType = t.enum({
      a: t.string(),
      b: t.number(),
    });
    expect(sumType.algebraicType).toEqual({
      tag: 'Sum',
      value: {
        variants: [
          { name: 'a', algebraicType: AlgebraicType.String },
          { name: 'b', algebraicType: AlgebraicType.F64 },
        ],
      },
    });
  });

  it('builds a ColumnBuilder with an index, unique constraint, and primary key', () => {
    const col = t.i32().index('btree').unique().primaryKey();
    expect(col.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(col.columnMetadata.isPrimaryKey).toBe(true);
    expect(col.columnMetadata.isUnique).toBe(true);
    expect(col.columnMetadata.indexType).toBe('btree');
    expect(col.columnMetadata.isAutoIncrement).toBe(false);
    expect(col.columnMetadata.isScheduleAt).toBe(false);
  });

  it('builds ColumnBuilders with the correct metadata', () => {
    const indexCol = t.i32().index('btree');
    const uniqueCol = t.i32().unique();
    const primaryKeyCol = t.i32().primaryKey();
    const autoIncCol = t.i32().autoInc();

    expect(indexCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(indexCol.columnMetadata.isPrimaryKey).toBe(false);
    expect(indexCol.columnMetadata.isUnique).toBe(false);
    expect(indexCol.columnMetadata.indexType).toBe('btree');
    expect(indexCol.columnMetadata.isAutoIncrement).toBe(false);
    expect(indexCol.columnMetadata.isScheduleAt).toBe(false);

    expect(uniqueCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(uniqueCol.columnMetadata.isPrimaryKey).toBe(false);
    expect(uniqueCol.columnMetadata.isUnique).toBe(true);
    expect(uniqueCol.columnMetadata.indexType).toBeUndefined();
    expect(uniqueCol.columnMetadata.isAutoIncrement).toBe(false);
    expect(uniqueCol.columnMetadata.isScheduleAt).toBe(false);

    expect(primaryKeyCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(primaryKeyCol.columnMetadata.isPrimaryKey).toBe(true);
    expect(primaryKeyCol.columnMetadata.isUnique).toBe(false);
    expect(primaryKeyCol.columnMetadata.indexType).toBeUndefined();
    expect(primaryKeyCol.columnMetadata.isAutoIncrement).toBe(false);
    expect(primaryKeyCol.columnMetadata.isScheduleAt).toBe(false);

    expect(autoIncCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(autoIncCol.columnMetadata.isPrimaryKey).toBe(false);
    expect(autoIncCol.columnMetadata.isUnique).toBe(false);
    expect(autoIncCol.columnMetadata.indexType).toBeUndefined();
    expect(autoIncCol.columnMetadata.isAutoIncrement).toBe(true);
    expect(autoIncCol.columnMetadata.isScheduleAt).toBe(false);
  });

  it('builds a ScheduleAt column with the correct type and metadata', () => {
    const col = t.scheduleAt();
    expect(col.typeBuilder.algebraicType).toEqual({
      tag: 'Sum',
      value: {
        variants: [
          {
            name: 'Interval',
            algebraicType: {
              tag: 'Product',
              value: {
                elements: [
                  {
                    name: '__time_duration_micros__',
                    algebraicType: AlgebraicType.I64,
                  },
                ],
              },
            },
          },
          {
            name: 'Time',
            algebraicType: {
              tag: 'Product',
              value: {
                elements: [
                  {
                    name: '__timestamp_micros_since_unix_epoch__',
                    algebraicType: AlgebraicType.I64,
                  },
                ],
              },
            },
          },
        ],
      },
    });
    expect(col.columnMetadata.isScheduleAt).toBe(true);
  });
});
