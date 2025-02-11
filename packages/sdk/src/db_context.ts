import type { SubscriptionBuilderImpl } from './subscription_builder_impl';

/**
 * Interface representing a database context.
 *
 * @template DBView - Type representing the database view.
 * @template Reducers - Type representing the reducers.
 * @template SetReducerFlags - Type representing the reducer flags collection.
 */
export interface DBContext<
  DBView = any,
  Reducers = any,
  SetReducerFlags = any,
> {
  db: DBView;
  reducers: Reducers;
  setReducerFlags: SetReducerFlags;
  isActive: boolean;

  /**
   * Creates a new subscription builder.
   *
   * @returns The subscription builder.
   */
  subscriptionBuilder(): SubscriptionBuilderImpl<
    DBView,
    Reducers,
    SetReducerFlags
  >;

  /**
   * Disconnects from the database.
   */
  disconnect(): void;
}
