import type { ClientDbView } from './db_view';
import type { ReducersView } from './reducers';
import type { UntypedRemoteModule } from './spacetime_module';
import type { SubscriptionBuilderImpl } from './subscription_builder_impl';

/**
 * Interface representing a database context.
 *
 * @template DbView - Type representing the database view.
 * @template ReducersDef - Type representing the reducers.
 */
export interface DbContext<RemoteModule extends UntypedRemoteModule> {
  db: ClientDbView<RemoteModule>;
  reducers: ReducersView<RemoteModule>;
  isActive: boolean;

  /**
   * Creates a new subscription builder.
   *
   * @returns The subscription builder.
   */
  subscriptionBuilder(): SubscriptionBuilderImpl<RemoteModule>;

  /**
   * Disconnects from the database.
   */
  disconnect(): void;
}
