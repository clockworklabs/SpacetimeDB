import { useSpacetimeDB, useTable } from '../../../src/solid';
import { tables, User } from '../module_bindings';
import { Infer } from '../../../src';

export default function UserPage() {
  const connection = useSpacetimeDB();
  const [users] = useTable(tables.user);

  const identityHex = connection.identity?.toHexString();
  const currentUser = users.find(
    (u: Infer<typeof User>) => u.identity.toHexString() === identityHex
  );

  return (
    <section class="bg-gray-100 text-gray-700 p-8">
      <h1 class="text-2xl font-bold">User Page</h1>

      {currentUser ? (
        <div class="mt-4">
          <p>
            <strong>Identity:</strong> {identityHex}
          </p>
          <p>
            <strong>Times Incremented:</strong>{' '}
            {currentUser.hasIncrementedCount}
          </p>
        </div>
      ) : (
        <p class="mt-4">No user record found. Are you connected?</p>
      )}
    </section>
  );
}
