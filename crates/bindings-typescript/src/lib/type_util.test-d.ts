import type { ConnectionId } from './connection_id';
import type { Identity } from './identity';
import type { Prettify } from './type_util';
import type { TimeDuration } from './time_duration';
import type { Timestamp } from './timestamp';
import type { Uuid } from './uuid';

declare const brand: unique symbol;

/**
 * A branded primitive: an intersection of a primitive with a marker object.
 * `Prettify` must leave it alone — mapping it produces a structural record of
 * `String`'s members that no longer satisfies `string`.
 */
type UserId = string & { readonly [brand]: 'UserId' };

declare const userId: Prettify<UserId>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _brandedIsStillAString: string = userId;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _brandedKeepsItsBrand: UserId = userId;

// Plain primitives round-trip unchanged.
declare const str: Prettify<string>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _str: string = str;
declare const num: Prettify<number>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _num: number = num;
declare const big: Prettify<bigint>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _big: bigint = big;

// Every SATS wrapper class is passed through as the class, not as a
// structural record of its members.
declare const identity: Prettify<Identity>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _identity: Identity = identity;
declare const connectionId: Prettify<ConnectionId>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _connectionId: ConnectionId = connectionId;
declare const timestamp: Prettify<Timestamp>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _timestamp: Timestamp = timestamp;
declare const timeDuration: Prettify<TimeDuration>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _timeDuration: TimeDuration = timeDuration;
declare const uuid: Prettify<Uuid>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _uuid: Uuid = uuid;

// Object types are still flattened.
type Intersected = { a: string } & { b: number };
declare const flattened: Prettify<Intersected>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _flattened: { a: string; b: number } = flattened;
