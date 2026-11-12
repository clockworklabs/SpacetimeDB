import { UnknownCallResultError } from 'spacetimedb';
import { DbConnection } from './module_bindings/index.js';

export class ClientApp {
  readonly connection: DbConnection;
  ready = false;
  outcome = 'idle';
  changes = 0;
  constructor(uri: string, database: string) {
    this.connection = DbConnection.builder()
      .withUri(uri)
      .withDatabaseName(database)
      .withAutomaticReconnect({ minDelayMs: 500, maxDelayMs: 1000 })
      .onDisconnect(() => {
        this.ready = false;
      })
      .build();
    this.connection.db.counter.onUpdate(() => {
      this.changes++;
    });
    this.connection
      .subscriptionBuilder()
      .onApplied(() => {
        this.ready = true;
      })
      .subscribe('SELECT * FROM counter');
  }
  get value(): number | null {
    return this.connection.db.counter.id.find(0)?.value ?? null;
  }
  add(delta: number): void {
    this.outcome = 'pending';
    this.connection.reducers.add({ delta }).then(
      () => {
        this.outcome = 'committed';
      },
      error => {
        this.outcome =
          error instanceof UnknownCallResultError ? 'unknown' : 'failed';
      }
    );
  }
  stop(): void {
    this.ready = false;
    this.connection.disconnect();
  }
}
