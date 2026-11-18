import BinaryWriter from '../src/lib/binary_writer';
import { Identity } from '../src/lib/identity';
import type { Infer } from '../src/lib/type_builders';
import { RowSizeHint, TableUpdateRows } from '../src/sdk/client_api/types';
import PlayerRow from '../test-app/src/module_bindings/player_table';
import { Point } from '../test-app/src/module_bindings/types';
import UnindexedPlayerRow from '../test-app/src/module_bindings/unindexed_player_table';
import UserRow from '../test-app/src/module_bindings/user_table';

export const anIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);
export const bobIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000b0b'
);
export const sallyIdentity = Identity.fromString(
  '000000000000000000000000000000000000000000000000000000000006a111'
);

export function encodePlayer(value: Infer<typeof PlayerRow>): Uint8Array {
  const writer = new BinaryWriter(1024);
  PlayerRow.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeUnindexedPlayer(
  value: Infer<typeof UnindexedPlayerRow>
): Uint8Array {
  const writer = new BinaryWriter(1024);
  UnindexedPlayerRow.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeUser(value: Infer<typeof UserRow>): Uint8Array {
  const writer = new BinaryWriter(1024);
  UserRow.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeCreatePlayerArgs(
  name: string,
  location: Infer<typeof Point>
): Uint8Array {
  const writer = new BinaryWriter(1024);
  writer.writeString(name);
  Point.serialize(writer, location);
  return writer.getBuffer();
}

// A single `Uint8Array` is one encoded row, or no rows if it's empty.
type EncodedRows = Uint8Array | Uint8Array[];

export function makeRowList(encodedRows: EncodedRows) {
  const rows = Array.isArray(encodedRows)
    ? encodedRows
    : encodedRows.length > 0
      ? [encodedRows]
      : [];
  let nextOffset = 0n;
  const offsets = rows.map(row => {
    const offset = nextOffset;
    nextOffset += BigInt(row.length);
    return offset;
  });
  return {
    sizeHint: RowSizeHint.RowOffsets(offsets),
    rowsData: new Uint8Array(rows.flatMap(row => [...row])),
  };
}

export function makeQueryRows(table: string, rows: EncodedRows) {
  return {
    tables: [
      {
        table,
        rows: makeRowList(rows),
      },
    ],
  };
}

export function makeQuerySetUpdate(
  querySetId: number,
  tableName: string,
  inserts: EncodedRows,
  deletes: EncodedRows = new Uint8Array()
) {
  return {
    querySetId: { id: querySetId },
    tables: [
      {
        tableName,
        rows: [
          TableUpdateRows.PersistentTable({
            inserts: makeRowList(inserts),
            deletes: makeRowList(deletes),
          }),
        ],
      },
    ],
  };
}
