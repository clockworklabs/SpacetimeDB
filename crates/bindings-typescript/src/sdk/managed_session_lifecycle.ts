/** Internal opt-in boundary used only by managed container sessions. */
export const INTERNAL_MANAGED_SESSION: unique symbol =
  Symbol('managed session');
export const INTERNAL_CLEAR_TABLE_CALLBACKS: unique symbol = Symbol(
  'clear table callbacks'
);

/** A local terminal outcome, never a claim that the server rolled a call back. */
export class ContainerSessionCallError extends Error {
  constructor(readonly outcome: 'not_sent' | 'unknown') {
    super(
      outcome === 'not_sent'
        ? 'Container session ended before this call was sent'
        : 'Container session ended without a confirmed call result; outcome unknown'
    );
    this.name = 'ContainerSessionCallError';
  }
}

export interface ManagedSessionLifecycle {
  enable(): void;
  seal(error: Error): void;
}
