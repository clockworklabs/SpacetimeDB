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
