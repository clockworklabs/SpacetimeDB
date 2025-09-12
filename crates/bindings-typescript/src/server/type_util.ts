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

export function set<T, F extends string, V>(
  x: T,
  t: { [k in F]: V }
): Set<T, F, V> {
  return { ...x, ...t };
}

type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;

export type DifferenceFromDefault<T, D> = Prettify<{
  [K in keyof T as K extends keyof D
    ? Equals<T[K], D[K]> extends true
      ? never
      : K
    : K]: T[K];
}>;
