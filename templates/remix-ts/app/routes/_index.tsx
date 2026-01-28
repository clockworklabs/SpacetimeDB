import { useState, useEffect } from 'react';
import type { MetaFunction } from '@remix-run/node';
import { tables, reducers } from '../../src/module_bindings';
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/react';

export const meta: MetaFunction = () => {
  return [
    { title: 'SpacetimeDB Remix App' },
    { name: 'description', content: 'A Remix app powered by SpacetimeDB' },
  ];
};

// Client-only component that uses SpacetimeDB hooks
function SpacetimeDBContent() {
  const [name, setName] = useState('');

  const conn = useSpacetimeDB();
  const { isActive: connected } = conn;

  // Subscribe to all people in the database
  // useTable returns [rows, isLoading] tuple
  const [people] = useTable(tables.person);

  const addReducer = useReducer(reducers.add);

  const addPerson = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !connected) return;

    // Call the add reducer with object syntax
    addReducer({ name: name });
    setName('');
  };

  return (
    <>
      <div style={{ marginBottom: '1rem' }}>
        Status:{' '}
        <strong style={{ color: connected ? 'green' : 'red' }}>
          {connected ? 'Connected' : 'Disconnected'}
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
        <h2>People ({people.length})</h2>
        {people.length === 0 ? (
          <p>No people yet. Add someone above!</p>
        ) : (
          <ul>
            {people.map((person, index) => (
              <li key={index}>{person.name}</li>
            ))}
          </ul>
        )}
      </div>
    </>
  );
}

export default function Index() {
  const [isClient, setIsClient] = useState(false);

  useEffect(() => {
    setIsClient(true);
  }, []);

  return (
    <main style={{ padding: '2rem', fontFamily: 'system-ui, sans-serif' }}>
      <h1>SpacetimeDB Remix App</h1>

      {isClient ? (
        <SpacetimeDBContent />
      ) : (
        <div style={{ marginBottom: '1rem' }}>
          Status: <strong style={{ color: 'gray' }}>Loading...</strong>
        </div>
      )}
    </main>
  );
}
