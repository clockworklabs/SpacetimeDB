'use client';

import { useState, useEffect } from 'react';
import { tables, reducers } from '../src/module_bindings';
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/react';
import type { PersonData } from '../lib/spacetimedb-server';

interface PersonListProps {
  initialPeople: PersonData[];
}

export function PersonList({ initialPeople }: PersonListProps) {
  const [name, setName] = useState('');
  const [isHydrated, setIsHydrated] = useState(false);

  const conn = useSpacetimeDB();
  const { isActive: connected } = conn;

  // Subscribe to all people in the database
  // useTable returns [rows, isLoading] tuple
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

    // Call the add reducer with object syntax
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
