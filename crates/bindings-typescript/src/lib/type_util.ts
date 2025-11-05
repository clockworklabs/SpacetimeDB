/**
 * Utility to make TS show cleaner types by flattening intersections.
 */
export type Prettify<T> = { [K in keyof T]: T[K] } & {};

type Builtin =
  | string | number | boolean | symbol | bigint | null | undefined
  | Function | Date | RegExp | Error
  | Map<any, any> | Set<any> | WeakMap<any, any> | WeakSet<any>
  | Promise<any>;

// Deep
export type _PrettifyDeep<T> =
  T extends Builtin ? T :
  T extends readonly [...infer _] ? { [K in keyof T]: PrettifyDeep<T[K]> } :
  T extends ReadonlyArray<infer U> ? ReadonlyArray<PrettifyDeep<U>> :
  T extends object ? Prettify<{ [K in keyof T]: PrettifyDeep<T[K]> }> :
  T;

export type PrettifyDeep<T> = T extends unknown ? _PrettifyDeep<T> : never;

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
  return { ...x, ...t };
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