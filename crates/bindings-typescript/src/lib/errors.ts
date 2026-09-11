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
  get name(): string {
    return 'SenderError';
  }
}

/**
 * An internal reducer error returned by the server runtime.
 */
export class InternalError extends Error {
  constructor(message: string) {
    super(message);
  }
  get name(): string {
    return 'InternalError';
  }
}

/** The call was not sent because the connection was not established. */
export class DisconnectedError extends Error {
  constructor(message: string = 'Not connected to SpacetimeDB') {
    super(message);
  }
  get name(): string {
    return 'DisconnectedError';
  }
}

/** The connection dropped before acknowledgement; the call may have run. */
export class UnknownCallResultError extends Error {
  constructor(
    message: string = 'Connection lost before the call was acknowledged; it may or may not have run'
  ) {
    super(message);
  }
  get name(): string {
    return 'UnknownCallResultError';
  }
}

/** The reconnect returned a different identity, ending automatic reconnection. */
export class IdentityChangedError extends Error {
  constructor(
    message: string = 'Reconnected with a different identity; the token was revoked or replaced'
  ) {
    super(message);
  }
  get name(): string {
    return 'IdentityChangedError';
  }
}
