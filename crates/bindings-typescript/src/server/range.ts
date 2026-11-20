// Registered globally, so every copy of this module shares it.
const RANGE_BRAND = Symbol.for('spacetimedb.Range');

/**
 * A class representing a range with optional lower and upper bounds.
 * This class is used to specify ranges for index scans in SpacetimeDB.
 *
 * The range can be defined with inclusive or exclusive bounds, or can be unbounded on either side.
 * @template T - The type of the values in the range.
 * @example
 * ```typescript
 * // Create a range from 10 (inclusive) to 20 (exclusive)
 * const range = new Range(
 *   { tag: 'included', value: 10 },
 *   { tag: 'excluded', value: 20 }
 * );
 * // Create an unbounded range
 * const unboundedRange = new Range();
 * ```
 */
export class Range<T> {
  /**
   * Recognise a `Range` built by another copy of this class.
   *
   * The package's entry points are bundled separately, so a module that imports
   * `Range` from `spacetimedb/server` and is run by the test harness in
   * `spacetimedb/server/test-utils` holds a different class from the one the
   * harness checks against. Without this, the harness would take the range for
   * a scalar key and fail to serialize it.
   */
  static [Symbol.hasInstance](value: unknown): boolean {
    return (
      typeof value === 'object' &&
      value !== null &&
      (value as { [RANGE_BRAND]?: unknown })[RANGE_BRAND] === true
    );
  }

  readonly [RANGE_BRAND] = true;
  #from: Bound<T>;
  #to: Bound<T>;
  public constructor(from?: Bound<T> | null, to?: Bound<T> | null) {
    this.#from = from ?? { tag: 'unbounded' };
    this.#to = to ?? { tag: 'unbounded' };
  }

  public get from(): Bound<T> {
    return this.#from;
  }
  public get to(): Bound<T> {
    return this.#to;
  }
}

/**
 * A type representing a bound in a range, which can be inclusive, exclusive, or unbounded.
 * - `included`: The bound is inclusive, meaning the value is part of the range.
 * - `excluded`: The bound is exclusive, meaning the value is not part of the range.
 * - `unbounded`: The bound is unbounded, meaning there is no limit in that direction.
 * @template T - The type of the value for the bound.
 * @example
 * ```typescript
 * // Inclusive bound
 * const inclusiveBound: Bound<number> = { tag: 'included', value: 10 };
 * // Exclusive bound
 * const exclusiveBound: Bound<number> = { tag: 'excluded', value: 20 };
 * // Unbounded bound
 * const unbounded: Bound<number> = { tag: 'unbounded' };
 * ```
 */
export type Bound<T> =
  | { tag: 'included'; value: T }
  | { tag: 'excluded'; value: T }
  | { tag: 'unbounded' };
