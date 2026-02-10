import BinaryWriter from '../src/lib/binary_writer';
import { Identity } from '../src/lib/identity';
import type { Infer } from '../src/lib/type_builders';
import { Player, Point, User } from '../test-app/src/module_bindings';
import * as ws from '../src/sdk/client_api';

export const anIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);
export const bobIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000b0b'
);
export const sallyIdentity = Identity.fromString(
  '000000000000000000000000000000000000000000000000000000000006a111'
);

export function encodePlayer(value: Infer<typeof Player>): Uint8Array {
  const writer = new BinaryWriter(1024);
  Player.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeUser(value: Infer<typeof User>): Uint8Array {
  const writer = new BinaryWriter(1024);
  User.serialize(writer, value);
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

export function makeRowList(rowsData: Uint8Array) {
  return {
    sizeHint: ws.RowSizeHint.FixedSize(0),
    rowsData,
  };
}

export function makeQueryRows(table: string, rowsData: Uint8Array) {
  return {
    tables: [
      {
        table,
        rows: makeRowList(rowsData),
      },
    ],
  };
}

export function makeQuerySetUpdate(
  querySetId: number,
  tableName: string,
  inserts: Uint8Array,
  deletes: Uint8Array = new Uint8Array()
) {
  return {
    querySetId: { id: querySetId },
    tables: [
      {
        tableName,
        rows: [
          ws.TableUpdateRows.PersistentTable({
            inserts: makeRowList(inserts),
            deletes: makeRowList(deletes),
          }),
        ],
      },
    ],
  };
}
