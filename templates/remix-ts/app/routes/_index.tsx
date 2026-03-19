import { useState, useEffect } from 'react';
import type { MetaFunction } from '@remix-run/node';
import { useLoaderData } from '@remix-run/react';
import { tables, reducers } from '../../src/module_bindings';
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/react';
import { fetchPeople, type PersonData } from '../lib/spacetimedb.server';

export const meta: MetaFunction = () => {
  return [
    { title: 'SpacetimeDB Remix App' },
    { name: 'description', content: 'A Remix app powered by SpacetimeDB' },
  ];
};

export async function loader() {
  try {
    const people = await fetchPeople();
    return { initialPeople: people };
  } catch (error) {
    // If server-side fetch fails, the client will still work
    console.error('Failed to fetch initial data:', error);
    return { initialPeople: [] as PersonData[] };
  }
}

// Client component that uses SpacetimeDB hooks for real-time updates
function PersonList({ initialPeople }: { initialPeople: PersonData[] }) {
  const [name, setName] = useState('');
  const [isHydrated, setIsHydrated] = useState(false);

  const conn = useSpacetimeDB();
  const { isActive: connected } = conn;

  // Subscribe to all people in the database
  const [people, isLoading] = useTable(tables.person);

  const addReducer = useReducer(reducers.add);

  // Once connected and loaded, we're hydrated with real-time data
  useEffect(() => {
    if (connected && !isLoading) {
      setIsHydrated(true);
    }
  }, [connected, isLoading]);

  // Use server-rendered data until client is hydrated with real-time data
  const displayPeople = isHydrated ? people : initialPeople;

  const addPerson = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !connected) return;

    addReducer({ name: name });
    setName('');
  };

  return (
    <>
      <div style={{ marginBottom: '1rem' }}>
        Status:{' '}
        <strong style={{ color: connected ? 'green' : 'red' }}>
          {connected ? 'Connected' : 'Connecting...'}
        </strong>
      </div>

      <form onSubmit={addPerson} style={{ marginBottom: '2rem' }}>
        <input
          type="text"
          placeholder="Enter name"
          value={name}
          onChange={e => setName(e.target.value)}
          style={{ padding: '0.5rem', marginRight: '0.5rem' }}
          disabled={!connected}
        />
        <button
          type="submit"
          style={{ padding: '0.5rem 1rem' }}
          disabled={!connected}
        >
          Add Person
        </button>
      </form>

      <div>
        <h2>People ({displayPeople.length})</h2>
        {displayPeople.length === 0 ? (
          <p>No people yet. Add someone above!</p>
        ) : (
          <ul>
            {displayPeople.map((person, index) => (
              <li key={index}>{person.name}</li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

export default function Index() {
  const { initialPeople } = useLoaderData<typeof loader>();
  const [isClient, setIsClient] = useState(false);

  useEffect(() => {
    setIsClient(true);
  }, []);

  return (
    <main style={{ padding: '2rem', fontFamily: 'system-ui, sans-serif' }}>
      <h1>SpacetimeDB Remix App</h1>

      {isClient ? (
        <PersonList initialPeople={initialPeople} />
      ) : (
        <>
          <div style={{ marginBottom: '1rem' }}>
            Status: <strong style={{ color: 'gray' }}>Loading...</strong>
          </div>
          <div>
            <h2>People ({initialPeople.length})</h2>
            {initialPeople.length === 0 ? (
              <p>No people yet. Add someone above!</p>
            ) : (
              <ul>
                {initialPeople.map((person, index) => (
                  <li key={index}>{person.name}</li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </main>
  );
}
