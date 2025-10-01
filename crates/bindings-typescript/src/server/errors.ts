export class SpacetimeError {
  public readonly code: number;
  public readonly message: string;
  constructor(code: number) {
    const proto = Object.getPrototypeOf(this);
    let cls;
    if (error_protoypes.has(proto)) {
      cls = proto.constructor;
      if (code !== cls.CODE)
        throw new TypeError(`invalid error code for ${cls.name}`);
    } else if (proto === SpacetimeError.prototype) {
      cls = errno_to_class.get(code);
      if (!cls) throw new RangeError(`unknown error code ${code}`);
    } else {
      throw new TypeError('cannot subclass SpacetimeError');
    }
    Object.setPrototypeOf(this, cls.prototype);
    this.code = cls.CODE;
    this.message = cls.MESSAGE;
  }
}

export class HostCallFailure extends SpacetimeError {
  static CODE = 1;
  static MESSAGE = 'ABI called by host returned an error';
  constructor() {
    super(HostCallFailure.CODE);
  }
}
export class NotInTransaction extends SpacetimeError {
  static CODE = 2;
  static MESSAGE = 'ABI call can only be made while in a transaction';
  constructor() {
    super(NotInTransaction.CODE);
  }
}
export class BsatnDecodeError extends SpacetimeError {
  static CODE = 3;
  static MESSAGE = "Couldn't decode the BSATN to the expected type";
  constructor() {
    super(BsatnDecodeError.CODE);
  }
}
export class NoSuchTable extends SpacetimeError {
  static CODE = 4;
  static MESSAGE = 'No such table';
  constructor() {
    super(NoSuchTable.CODE);
  }
}
export class NoSuchIndex extends SpacetimeError {
  static CODE = 5;
  static MESSAGE = 'No such index';
  constructor() {
    super(NoSuchIndex.CODE);
  }
}
export class NoSuchIter extends SpacetimeError {
  static CODE = 6;
  static MESSAGE = 'The provided row iterator is not valid';
  constructor() {
    super(NoSuchIter.CODE);
  }
}
export class NoSuchConsoleTimer extends SpacetimeError {
  static CODE = 7;
  static MESSAGE = 'The provided console timer does not exist';
  constructor() {
    super(NoSuchConsoleTimer.CODE);
  }
}
export class NoSuchBytes extends SpacetimeError {
  static CODE = 8;
  static MESSAGE = 'The provided bytes source or sink is not valid';
  constructor() {
    super(NoSuchBytes.CODE);
  }
}
export class NoSpace extends SpacetimeError {
  static CODE = 9;
  static MESSAGE = 'The provided sink has no more space left';
  constructor() {
    super(NoSpace.CODE);
  }
}
export class BufferTooSmall extends SpacetimeError {
  static CODE = 11;
  static MESSAGE = 'The provided buffer is not large enough to store the data';
  constructor() {
    super(BufferTooSmall.CODE);
  }
}
export class UniqueAlreadyExists extends SpacetimeError {
  static CODE = 12;
  static MESSAGE = 'Value with given unique identifier already exists';
  constructor() {
    super(UniqueAlreadyExists.CODE);
  }
}
export class ScheduleAtDelayTooLong extends SpacetimeError {
  static CODE = 13;
  static MESSAGE = 'Specified delay in scheduling row was too long';
  constructor() {
    super(ScheduleAtDelayTooLong.CODE);
  }
}
export class IndexNotUnique extends SpacetimeError {
  static CODE = 14;
  static MESSAGE = 'The index was not unique';
  constructor() {
    super(IndexNotUnique.CODE);
  }
}
export class NoSuchRow extends SpacetimeError {
  static CODE = 15;
  static MESSAGE = 'The row was not found, e.g., in an update call';
  constructor() {
    super(NoSuchRow.CODE);
  }
}
export class AutoIncOverflow extends SpacetimeError {
  static CODE = 16;
  static MESSAGE = 'The auto-increment sequence overflowed';
  constructor() {
    super(AutoIncOverflow.CODE);
  }
}

const error_subclasses = [
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

const error_protoypes = new Set(error_subclasses.map(cls => cls.prototype));

const errno_to_class = new Map(
  error_subclasses.map(cls => [cls.CODE as number, cls])
);
