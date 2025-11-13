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
  constructor(code: number) {
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
    this.message = cls.MESSAGE;
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

/**
 * A generic error class for unknown error codes.
 */
export class HostCallFailure extends SpacetimeHostError {
  static CODE = 1;
  static MESSAGE = 'ABI called by host returned an error';
  constructor() {
    super(HostCallFailure.CODE);
  }
}

/**
 * Error indicating that an ABI call was made outside of a transaction.
 */
export class NotInTransaction extends SpacetimeHostError {
  static CODE = 2;
  static MESSAGE = 'ABI call can only be made while in a transaction';
  constructor() {
    super(NotInTransaction.CODE);
  }
}

/**
 * Error indicating that BSATN decoding failed.
 * This typically means that the data could not be decoded to the expected type.
 */
export class BsatnDecodeError extends SpacetimeHostError {
  static CODE = 3;
  static MESSAGE = "Couldn't decode the BSATN to the expected type";
  constructor() {
    super(BsatnDecodeError.CODE);
  }
}

/**
 * Error indicating that a specified table does not exist.
 */
export class NoSuchTable extends SpacetimeHostError {
  static CODE = 4;
  static MESSAGE = 'No such table';
  constructor() {
    super(NoSuchTable.CODE);
  }
}

/**
 * Error indicating that a specified index does not exist.
 */
export class NoSuchIndex extends SpacetimeHostError {
  static CODE = 5;
  static MESSAGE = 'No such index';
  constructor() {
    super(NoSuchIndex.CODE);
  }
}

/**
 * Error indicating that a specified row iterator is not valid.
 */
export class NoSuchIter extends SpacetimeHostError {
  static CODE = 6;
  static MESSAGE = 'The provided row iterator is not valid';
  constructor() {
    super(NoSuchIter.CODE);
  }
}

/**
 * Error indicating that a specified console timer does not exist.
 */
export class NoSuchConsoleTimer extends SpacetimeHostError {
  static CODE = 7;
  static MESSAGE = 'The provided console timer does not exist';
  constructor() {
    super(NoSuchConsoleTimer.CODE);
  }
}

/**
 * Error indicating that a specified bytes source or sink is not valid.
 */
export class NoSuchBytes extends SpacetimeHostError {
  static CODE = 8;
  static MESSAGE = 'The provided bytes source or sink is not valid';
  constructor() {
    super(NoSuchBytes.CODE);
  }
}

/**
 * Error indicating that a provided sink has no more space left.
 */
export class NoSpace extends SpacetimeHostError {
  static CODE = 9;
  static MESSAGE = 'The provided sink has no more space left';
  constructor() {
    super(NoSpace.CODE);
  }
}

/**
 * Error indicating that there is no more space in the database.
 */
export class BufferTooSmall extends SpacetimeHostError {
  static CODE = 11;
  static MESSAGE = 'The provided buffer is not large enough to store the data';
  constructor() {
    super(BufferTooSmall.CODE);
  }
}

/**
 * Error indicating that a value with a given unique identifier already exists.
 */
export class UniqueAlreadyExists extends SpacetimeHostError {
  static CODE = 12;
  static MESSAGE = 'Value with given unique identifier already exists';
  constructor() {
    super(UniqueAlreadyExists.CODE);
  }
}

/**
 * Error indicating that the specified delay in scheduling a row was too long.
 */
export class ScheduleAtDelayTooLong extends SpacetimeHostError {
  static CODE = 13;
  static MESSAGE = 'Specified delay in scheduling row was too long';
  constructor() {
    super(ScheduleAtDelayTooLong.CODE);
  }
}

/**
 * Error indicating that an index was not unique when it was expected to be.
 */
export class IndexNotUnique extends SpacetimeHostError {
  static CODE = 14;
  static MESSAGE = 'The index was not unique';
  constructor() {
    super(IndexNotUnique.CODE);
  }
}

/**
 * Error indicating that an index was not unique when it was expected to be.
 */
export class NoSuchRow extends SpacetimeHostError {
  static CODE = 15;
  static MESSAGE = 'The row was not found, e.g., in an update call';
  constructor() {
    super(NoSuchRow.CODE);
  }
}

/**
 * Error indicating that an auto-increment sequence has overflowed.
 */
export class AutoIncOverflow extends SpacetimeHostError {
  static CODE = 16;
  static MESSAGE = 'The auto-increment sequence overflowed';
  constructor() {
    super(AutoIncOverflow.CODE);
  }
}

/**
 * List of all SpacetimeError subclasses.
 */
const errorSubclasses = [
  HostCallFailure,
  NotInTransaction,
  BsatnDecodeError,
  NoSuchTable,
  NoSuchIndex,
  NoSuchIter,
  NoSuchConsoleTimer,
  NoSuchBytes,
  NoSpace,
  BufferTooSmall,
  UniqueAlreadyExists,
  ScheduleAtDelayTooLong,
  IndexNotUnique,
  NoSuchRow,
];

/**
 * Set of prototypes of all SpacetimeError subclasses for quick lookup.
 */
const errorProtoypes = new Set(errorSubclasses.map(cls => cls.prototype));

/**
 * Map from error codes to their corresponding SpacetimeError subclass.
 */
const errnoToClass = new Map(
  errorSubclasses.map(cls => [cls.CODE as number, cls])
);
