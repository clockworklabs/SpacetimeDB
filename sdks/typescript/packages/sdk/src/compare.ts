/**
 * Compares the current instance with another object of the same type.
 */
interface Comparable<T> {
  /**
   * Compares the current instance with another object of the same type.
   * Returns:
   * - 0 : is `===` to the other object
   * - 1 : is `>` to the other object
   * - -1 : `<` to the other object
   * @param other
   */
  compareTo(other: T): number;

  /**
   * Checks if the current instance is `===` to another object of the same type.
   * @param other
   */
  isEqual(other: T): boolean;
}
