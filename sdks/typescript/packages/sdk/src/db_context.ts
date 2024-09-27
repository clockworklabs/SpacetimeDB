import type { DBConnectionBase } from './spacetimedb.ts';

export class DbContext<DbView, ReducerView> {
  #client: DBConnectionBase;

  readonly db: DbView;
  readonly reducers: ReducerView;

  readonly identity: string | undefined;
  readonly address: string | undefined;

  isActive: boolean;

  constructor(client: DBConnectionBase, db: DbView, reducers: ReducerView) {
    this.#client = client;
    this.db = db;
    this.reducers = reducers;
    this.isActive = client.isActive;

    this.#client.on('connected', () => {
      this.isActive = true;
    });

    this.#client.on('disconnected', () => {
      this.isActive = false;
    });
  }

  // TODO: Later
  onSubscriptionApplied(callback: () => void): void {}
}
