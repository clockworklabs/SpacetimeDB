import type { ConnectionId } from "./connection_id";
import type { Identity } from "./identity";
import type { ScheduleAt } from "./schedule_at";
import type { TimeDuration } from "./time_duration";
import type { Timestamp } from "./timestamp";

type DoNotPrettify = Identity | ConnectionId | Timestamp | TimeDuration | ScheduleAt;

/**
 * Utility to make TS show cleaner types by flattening intersections.
 */
export type Prettify<T> =
  T extends DoNotPrettify ? T : { [K in keyof T]: T[K] } & {};

/**
 * Helper function to sets a field in an object
 */
export type SetField<T, F extends string, V> = Prettify<
  Omit<T, F> & { [K in F]: V }
>;

/**
 * Sets a field in an object
 * @param x The original object
 * @param t The object containing the field to set
 * @returns A new object with the field set
 */
export function set<T, F extends string, V>(
  x: T,
  t: { [k in F]: V }
): SetField<T, F, V> {
  return { ...x, ...t } as SetField<T, F, V>;
}

/**
 * Helper to extract the value types from an object type
 */
export type Values<T> = T[keyof T];

/**
 * A helper type to collapse a tuple into a single type if it has only one element.
 */
export type CollapseTuple<A extends any[]> = A extends [infer T] ? T : A;

type CamelCaseImpl<S extends string> =
  S extends `${infer Head}_${infer Tail}` ? `${Head}${Capitalize<CamelCaseImpl<Tail>>}` :
  S extends `${infer Head}-${infer Tail}` ? `${Head}${Capitalize<CamelCaseImpl<Tail>>}` :
  S;

/**
 * Convert "Some_identifier-name" -> "someIdentifierName"
 * - No spaces; allowed separators: "_" and "-"
 * - Normalizes the *first* character to lowercase (e.g. "User_Name" -> "userName")
 */
export type CamelCase<S extends string> = Uncapitalize<CamelCaseImpl<S>>;