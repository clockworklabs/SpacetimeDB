import type { DbView } from '../server/db_view';
import type { UntypedSchemaDef } from '../server/schema';
import type { ReducersView, SetReducerFlags, UntypedReducersDef } from './reducers';
import type { SubscriptionBuilderImpl } from './subscription_builder_impl';

/**
 * Interface representing a database context.
 *
 * @template DbView - Type representing the database view.
 * @template ReducersDef - Type representing the reducers.
 * @template SetReducerFlags - Type representing the reducer flags collection.
 */
export interface DbContext<SchemaDef extends UntypedSchemaDef, ReducersDef extends UntypedReducersDef> {
  db: DbView<SchemaDef>;
  reducers: ReducersView<ReducersDef>;
  setReducerFlags: SetReducerFlags<ReducersDef>;
  isActive: boolean;

  /**
   * Creates a new subscription builder.
   *
   * @returns The subscription builder.
   */
  subscriptionBuilder(): SubscriptionBuilderImpl<
    SchemaDef,
    ReducersDef
  >;

  /**
   * Disconnects from the database.
   */
  disconnect(): void;
}
