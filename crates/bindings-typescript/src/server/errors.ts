/**
 * Base class for all Spacetime host errors (i.e. errors that may be thrown
 * by database functions).
 *
 * Instances of SpacetimeError can be created with just an error code,
 * which will return the appropriate subclass instance.
 */
export class SpacetimeHostError extends Error {
  public readonly code: number;
  public readonly message: string;
  constructor(code: number, message?: string) {
    super();
    const proto = Object.getPrototypeOf(this);
    let cls;
    if (errorProtoypes.has(proto)) {
      cls = proto.constructor;
      if (code !== cls.CODE)
        throw new TypeError(`invalid error code for ${cls.name}`);
    } else if (proto === SpacetimeHostError.prototype) {
      cls = errnoToClass.get(code);
      if (!cls) throw new RangeError(`unknown error code ${code}`);
    } else {
      throw new TypeError('cannot subclass SpacetimeError');
    }
    Object.setPrototypeOf(this, cls.prototype);
    this.code = cls.CODE;
    this.message = message ?? cls.MESSAGE;
  }
  get name(): string {
    return errnoToClass.get(this.code)?.name ?? 'SpacetimeHostError';
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
  HostCallFailure: [1, 'ABI called by host returned an error'],

  /**
   * Error indicating that an ABI call was made outside of a transaction.
   */
  NotInTransaction: [2, 'ABI call can only be made while in a transaction'],

  /**
   * Error indicating that BSATN decoding failed.
   * This typically means that the data could not be decoded to the expected type.
   */
  BsatnDecodeError: [3, "Couldn't decode the BSATN to the expected type"],

  /**
   * Error indicating that a specified table does not exist.
   */
  NoSuchTable: [4, 'No such table'],

  /**
   * Error indicating that a specified index does not exist.
   */
  NoSuchIndex: [5, 'No such index'],

  /**
   * Error indicating that a specified row iterator is not valid.
   */
  NoSuchIter: [6, 'The provided row iterator is not valid'],

  /**
   * Error indicating that a specified console timer does not exist.
   */
  NoSuchConsoleTimer: [7, 'The provided console timer does not exist'],

  /**
   * Error indicating that a specified bytes source or sink is not valid.
   */
  NoSuchBytes: [8, 'The provided bytes source or sink is not valid'],

  /**
   * Error indicating that a provided sink has no more space left.
   */
  NoSpace: [9, 'The provided sink has no more space left'],

  /**
   * Error indicating that there is no more space in the database.
   */
  BufferTooSmall: [
    11,
    'The provided buffer is not large enough to store the data',
  ],

  /**
   * Error indicating that a value with a given unique identifier already exists.
   */
  UniqueAlreadyExists: [
    12,
    'Value with given unique identifier already exists',
  ],

  /**
   * Error indicating that the specified delay in scheduling a row was too long.
   */
  ScheduleAtDelayTooLong: [
    13,
    'Specified delay in scheduling row was too long',
  ],

  /**
   * Error indicating that an index was not unique when it was expected to be.
   */
  IndexNotUnique: [14, 'The index was not unique'],

  /**
   * Error indicating that an index was not unique when it was expected to be.
   */
  NoSuchRow: [15, 'The row was not found, e.g., in an update call'],

  /**
   * Error indicating that an auto-increment sequence has overflowed.
   */
  AutoIncOverflow: [16, 'The auto-increment sequence overflowed'],

  WouldBlockTransaction: [
    17,
    'Attempted async or blocking op while holding open a transaction',
  ],

  TransactionNotAnonymous: [
    18,
    'Not in an anonymous transaction. Called by a reducer?',
  ],

  TransactionIsReadOnly: [
    19,
    'ABI call can only be made while within a mutable transaction',
  ],

  TransactionIsMut: [
    20,
    'ABI call can only be made while within a read-only transaction',
  ],

  HttpError: [21, 'The HTTP request failed'],
} as const;

function mapEntries<const T extends Record<string, any>, U>(
  x: T,
  f: (key: keyof T, value: T[keyof T]) => U
): { [k in keyof T]: U } {
  return Object.fromEntries(
    Object.entries(x).map(([k, v]) => [k, f(k, v)])
  ) as any;
}

export const errors = Object.freeze(
  mapEntries(errorData, (name, [code, message]) =>
    Object.defineProperty(
      class extends SpacetimeHostError {
        static CODE = code;
        static MESSAGE = message;
        constructor() {
          super(code);
        }
      },
      'name',
      { value: name, writable: false }
    )
  )
);

/**
 * Set of prototypes of all SpacetimeError subclasses for quick lookup.
 */
const errorProtoypes = new Set(Object.values(errors).map(cls => cls.prototype));

/**
 * Map from error codes to their corresponding SpacetimeError subclass.
 */
const errnoToClass = new Map(
  Object.values(errors).map(cls => [cls.CODE as number, cls])
);
