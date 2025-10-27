import { useEffect } from 'react';
import { useSpacetimeDB, useTable } from '../../../src/react';
import { DbConnection, User } from '../module_bindings';

export default function UserPage() {
  const stdb = useSpacetimeDB<DbConnection>();
  const { rows: users } = useTable<DbConnection, User>('user');

  useEffect(() => {
    if (!stdb.isActive) return;

    const sub = stdb
      .subscriptionBuilder()
      .onError((err: any) => console.error('User subscription error:', err))
      .onApplied(() => console.log('User subscription applied'))
      .subscribe('SELECT * FROM user');

    return () => {
      sub.unsubscribeThen(() => console.log('User subscription cleaned up'));
    };
  }, [stdb.isActive]);

  const identityHex = stdb.identity?.toHexString();
  const currentUser = users.find(
    (u: User) => u.identity.toHexString() === identityHex
  );

  return (
    <div>
      <h1>User Page</h1>
      {currentUser ? (
        <>
          <p>
            <strong>Identity:</strong> {identityHex}
          </p>
          <p>
            <strong>Times Incremented:</strong>{' '}
            {currentUser.hasIncrementedCount}
          </p>
        </>
      ) : (
        <p>No user record found. Are you connected?</p>
      )}
    </div>
  );
}
