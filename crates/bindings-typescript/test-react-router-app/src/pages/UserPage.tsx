import { useSpacetimeDB, useTable } from '../../../src/react';
import { tables, User } from '../module_bindings';
import { Infer } from '../../../src';

export default function UserPage() {
  const connection = useSpacetimeDB();
  const users = useTable(tables.user);

  const identityHex = connection.identity?.toHexString();
  const currentUser = users.find(
    (u: Infer<typeof User>) => u.identity.toHexString() === identityHex
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
