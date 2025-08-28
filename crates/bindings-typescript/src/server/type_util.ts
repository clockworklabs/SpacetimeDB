
/**
 * Utility to make TS show cleaner types by flattening intersections.
 */
export type Prettify<T> = { [K in keyof T]: T[K] } & {};