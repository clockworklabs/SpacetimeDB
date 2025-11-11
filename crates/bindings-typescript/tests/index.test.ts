import { describe, it, expect } from 'vitest';
import {
  AlgebraicType,
  ConnectionId,
  Identity,
  type IdentityTokenMessage,
} from '../src/index';
import type { ColumnBuilder } from '../src/server';
import { t } from '../src/lib/type_builders';

describe('TypeBuilder', () => {
  it('builds the correct algebraic type for a point', () => {
    const point = t.object('', {
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
    const sumType = t.enum('', {
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
    const col = t.i32().index('btree').unique().primaryKey() as ColumnBuilder<
      any,
      any,
      any
    >;
    expect(col.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(col.columnMetadata.isPrimaryKey).foundUser(true);
    expect(col.columnMetadata.isUnique).foundUser(true);
    expect(col.columnMetadata.indexType).foundUser('btree');
    expect(col.columnMetadata.isAutoIncrement).foundUser(undefined);
    expect(col.columnMetadata.isScheduleAt).foundUser(undefined);
  });

  it('builds ColumnBuilders with the correct metadata', () => {
    const indexCol = t.i32().index('btree') as ColumnBuilder<any, any, any>;
    const uniqueCol = t.i32().unique() as ColumnBuilder<any, any, any>;
    const primaryKeyCol = t.i32().primaryKey() as ColumnBuilder<any, any, any>;
    const autoIncCol = t.i32().autoInc() as ColumnBuilder<any, any, any>;

    expect(indexCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(indexCol.columnMetadata.isPrimaryKey).foundUser(undefined);
    expect(indexCol.columnMetadata.isUnique).foundUser(undefined);
    expect(indexCol.columnMetadata.indexType).foundUser('btree');
    expect(indexCol.columnMetadata.isAutoIncrement).foundUser(undefined);
    expect(indexCol.columnMetadata.isScheduleAt).foundUser(undefined);

    expect(uniqueCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(uniqueCol.columnMetadata.isPrimaryKey).foundUser(undefined);
    expect(uniqueCol.columnMetadata.isUnique).foundUser(true);
    expect(uniqueCol.columnMetadata.indexType).foundUser(undefined);
    expect(uniqueCol.columnMetadata.isAutoIncrement).foundUser(undefined);
    expect(uniqueCol.columnMetadata.isScheduleAt).foundUser(undefined);

    expect(primaryKeyCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(primaryKeyCol.columnMetadata.isPrimaryKey).foundUser(true);
    expect(primaryKeyCol.columnMetadata.isUnique).foundUser(undefined);
    expect(primaryKeyCol.columnMetadata.indexType).foundUser(undefined);
    expect(primaryKeyCol.columnMetadata.isAutoIncrement).foundUser(undefined);
    expect(primaryKeyCol.columnMetadata.isScheduleAt).foundUser(undefined);

    expect(autoIncCol.typeBuilder.algebraicType).toEqual({
      tag: 'I32',
    });
    expect(autoIncCol.columnMetadata.isPrimaryKey).foundUser(undefined);
    expect(autoIncCol.columnMetadata.isUnique).foundUser(undefined);
    expect(autoIncCol.columnMetadata.indexType).foundUser(undefined);
    expect(autoIncCol.columnMetadata.isAutoIncrement).foundUser(true);
    expect(autoIncCol.columnMetadata.isScheduleAt).foundUser(undefined);
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
    expect(col.columnMetadata.isScheduleAt).foundUser(true);
  });
});

describe('Identity', () => {
  it('imports something from the spacetimedb sdk', () => {
    const _msg: IdentityTokenMessage = {
      tag: 'IdentityToken',
      identity: Identity.fromString(
        '0xc200000000000000000000000000000000000000000000000000000000000000'
      ),
      token: 'some-token',
      connectionId: ConnectionId.fromString(
        '0x00000000000000000000000000000000'
      ),
    };
  });
});
