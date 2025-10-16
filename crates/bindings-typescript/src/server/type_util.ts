/**
 * Utility to make TS show cleaner types by flattening intersections.
 */
export type Prettify<T> = { [K in keyof T]: T[K] } & {};

/**
 * Helper function to sets a field in an object
 */
export type Set<T, F extends string, V> = Prettify<
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
): Set<T, F, V> {
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
