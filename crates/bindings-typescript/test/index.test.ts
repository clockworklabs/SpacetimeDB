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
    expect(col.columnMetadata.isAutoIncrement).toBe(undefined);
    expect(col.columnMetadata.isScheduleAt).toBe(undefined);
  });

  it('builds ColumnBuilders with the correct metadata', () => {
    const indexCol = t.i32().index('btree');
    const uniqueCol = t.i32().unique();
    const primaryKeyCol = t.i32().primaryKey();
    const autoIncCol = t.i32().autoInc();

    expect(indexCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(indexCol.columnMetadata.isPrimaryKey).toBe(undefined);
    expect(indexCol.columnMetadata.isUnique).toBe(undefined);
    expect(indexCol.columnMetadata.indexType).toBe('btree');
    expect(indexCol.columnMetadata.isAutoIncrement).toBe(undefined);
    expect(indexCol.columnMetadata.isScheduleAt).toBe(undefined);

    expect(uniqueCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(uniqueCol.columnMetadata.isPrimaryKey).toBe(undefined);
    expect(uniqueCol.columnMetadata.isUnique).toBe(true);
    expect(uniqueCol.columnMetadata.indexType).toBe(undefined);
    expect(uniqueCol.columnMetadata.isAutoIncrement).toBe(undefined);
    expect(uniqueCol.columnMetadata.isScheduleAt).toBe(undefined);

    expect(primaryKeyCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(primaryKeyCol.columnMetadata.isPrimaryKey).toBe(true);
    expect(primaryKeyCol.columnMetadata.isUnique).toBe(undefined);
    expect(primaryKeyCol.columnMetadata.indexType).toBe(undefined);
    expect(primaryKeyCol.columnMetadata.isAutoIncrement).toBe(undefined);
    expect(primaryKeyCol.columnMetadata.isScheduleAt).toBe(undefined);

    expect(autoIncCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(autoIncCol.columnMetadata.isPrimaryKey).toBe(undefined);
    expect(autoIncCol.columnMetadata.isUnique).toBe(undefined);
    expect(autoIncCol.columnMetadata.indexType).toBe(undefined);
    expect(autoIncCol.columnMetadata.isAutoIncrement).toBe(true);
    expect(autoIncCol.columnMetadata.isScheduleAt).toBe(undefined);
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
