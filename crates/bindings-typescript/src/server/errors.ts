/**
 * Base class for all Spacetime host errors (i.e. errors that may be thrown
 * by database functions).
 */
export class SpacetimeHostError extends Error {
  constructor(message: string) {
    super(message);
  }
  get name(): string {
    return 'SpacetimeHostError';
  }
}

/**
 * An error thrown by a reducer that indicates a problem to the sender.
 *
 * When this error is thrown by a reducer, the sender will be notified
 * that the reducer failed gracefully with the given message.
 */
export class SenderError extends Error {
  constructor(message: string) {
    super(message);
  }
  get name() {
    return 'SenderError';
  }
}

const errorData = {
  /**
   * A generic error class for unknown error codes.
   */
  HostCallFailure: 1,

  /**
   * Error indicating that an ABI call was made outside of a transaction.
   */
  NotInTransaction: 2,

  /**
   * Error indicating that BSATN decoding failed.
   * This typically means that the data could not be decoded to the expected type.
   */
  BsatnDecodeError: 3,

  /**
   * Error indicating that a specified table does not exist.
   */
  NoSuchTable: 4,

  /**
   * Error indicating that a specified index does not exist.
   */
  NoSuchIndex: 5,

  /**
   * Error indicating that a specified row iterator is not valid.
   */
  NoSuchIter: 6,

  /**
   * Error indicating that a specified console timer does not exist.
   */
  NoSuchConsoleTimer: 7,

  /**
   * Error indicating that a specified bytes source or sink is not valid.
   */
  NoSuchBytes: 8,

  /**
   * Error indicating that a provided sink has no more space left.
   */
  NoSpace: 9,

  /**
   * Error indicating that there is no more space in the database.
   */
  BufferTooSmall: 11,

  /**
   * Error indicating that a value with a given unique identifier already exists.
   */
  UniqueAlreadyExists: 12,

  /**
   * Error indicating that the specified delay in scheduling a row was too long.
   */
  ScheduleAtDelayTooLong: 13,

  /**
   * Error indicating that an index was not unique when it was expected to be.
   */
  IndexNotUnique: 14,

  /**
   * Error indicating that an index was not unique when it was expected to be.
   */
  NoSuchRow: 15,

  /**
   * Error indicating that an auto-increment sequence has overflowed.
   */
  AutoIncOverflow: 16,

  WouldBlockTransaction: 17,

  TransactionNotAnonymous: 18,

  TransactionIsReadOnly: 19,

  TransactionIsMut: 20,

  HttpError: 21,
};

function mapEntries<const T extends Record<string, any>, U>(
  x: T,
  f: (key: keyof T, value: T[keyof T]) => U
): { [k in keyof T]: U } {
  return Object.fromEntries(
    Object.entries(x).map(([k, v]) => [k, f(k, v)])
  ) as any;
}

/**
 * Map from error codes to their corresponding SpacetimeError subclass.
 */
const errnoToClass = new Map<number, new (msg: string) => Error>();

export const errors = Object.freeze(
  mapEntries(errorData, (name, code) => {
    const cls = Object.defineProperty(
      class extends SpacetimeHostError {
        get name() {
          return name;
        }
      },
      'name',
      { value: name, writable: false }
    );
    errnoToClass.set(code, cls);
    return cls;
  })
);

export function getErrorConstructor(code: number): new (msg: string) => Error {
  return errnoToClass.get(code) ?? SpacetimeHostError;
}
