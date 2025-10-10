import { AlgebraicType, BinaryWriter, Identity } from '../src';
import { Player, Point, User } from '../test-app/src/module_bindings';

export const anIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000069'
);
export const bobIdentity = Identity.fromString(
  '0000000000000000000000000000000000000000000000000000000000000b0b'
);
export const sallyIdentity = Identity.fromString(
  '000000000000000000000000000000000000000000000000000000000006a111'
);

export function encodePlayer(value: Player): Uint8Array {
  const writer = new BinaryWriter(1024);
  Player.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeUser(value: User): Uint8Array {
  const writer = new BinaryWriter(1024);
  User.serialize(writer, value);
  return writer.getBuffer();
}

export function encodeCreatePlayerArgs(
  name: string,
  location: Point
): Uint8Array {
  const writer = new BinaryWriter(1024);
  AlgebraicType.serializeValue(writer, AlgebraicType.String, name);
  Point.serialize(writer, location);
  return writer.getBuffer();
}
