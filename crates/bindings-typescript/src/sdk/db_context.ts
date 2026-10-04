import type { ClientDbView } from './db_view';
import type { ReducersView } from './reducers';
import type { UntypedRemoteModuleDecl } from './spacetime_module';
import type { SubscriptionBuilderBase } from './subscription_builder_impl';

/**
 * Interface representing a database context.
 *
 * @template DbView - Type representing the database view.
 * @template ReducersDecl - Type representing the reducers.
 */
export interface DbContext<RemoteModuleDecl extends UntypedRemoteModuleDecl> {
  db: ClientDbView<RemoteModuleDecl>;
  reducers: ReducersView<RemoteModuleDecl>;
  isActive: boolean;

  /**
   * Creates a new subscription builder.
   *
   * @returns The subscription builder.
   */
  subscriptionBuilder(): SubscriptionBuilderBase<RemoteModuleDecl>;

  /**
   * Disconnects from the database.
   */
  disconnect(): void;
}
