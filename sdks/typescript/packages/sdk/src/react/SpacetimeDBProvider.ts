import { DbConnectionBuilder, type DbConnectionImpl } from '../index';
import React from 'react';
import { SpacetimeDBContext } from './useSpacetimeDB';

export interface SpacetimeDBProviderProps<
  DbConnection extends DbConnectionImpl,
  ErrorContext,
  SubscriptionEventContext,
> {
  connectionBuilder: DbConnectionBuilder<
    DbConnection,
    ErrorContext,
    SubscriptionEventContext
  >;
  children?: React.ReactNode;
}

export function SpacetimeDBProvider<
  DbConnection extends DbConnectionImpl,
  ErrorContext,
  SubscriptionEventContext,
>({
  connectionBuilder,
  children,
}: SpacetimeDBProviderProps<
  DbConnection,
  ErrorContext,
  SubscriptionEventContext
>): React.JSX.Element {
  return React.createElement(
    SpacetimeDBContext.Provider,
    { value: connectionBuilder.build() }, // May need to modify this to do it lazily in server-side rendering
    children
  );
}
