import { useEffect, useState, type ComponentProps } from 'react';
import { reducers, tables } from '../module_bindings';
import type { PersonData } from '../lib/spacetimedb-server';
import { useReducer, useSpacetimeDB, useTable } from 'spacetimedb/react';

interface PersonListProps {
  initialPeople: PersonData[];
}

export function PersonList({ initialPeople }: PersonListProps) {
  const [name, setName] = useState('');
  const [isHydrated, setIsHydrated] = useState(false);

  const connection = useSpacetimeDB();
  const { isActive: connected } = connection;

  const [people, isLoading] = useTable(tables.person);
  const addPersonReducer = useReducer(reducers.add);

  useEffect(() => {
    if (connected && !isLoading) {
      setIsHydrated(true);
    }
  }, [connected, isLoading]);

  const displayPeople = isHydrated ? people : initialPeople;

  const handleSubmit: NonNullable<ComponentProps<'form'>['onSubmit']> = event => {
    event.preventDefault();

    const trimmedName = name.trim();
    if (!trimmedName || !connected) {
      return;
    }

    addPersonReducer({ name: trimmedName });
    setName('');
  };

  return (
    <section className="live-app">
      <div className="status-row">
        <strong>Status:</strong>{' '}
        <span className={connected ? 'status-live' : 'status-pending'}>
          {connected ? 'Connected' : 'Connecting...'}
        </span>
        <span className="status-note">
          {isHydrated ? 'Live subscription active' : 'Showing server snapshot'}
        </span>
      </div>

      <form className="person-form" onSubmit={handleSubmit}>
        <label className="sr-only" htmlFor="person-name">
          Name
        </label>
        <input
          id="person-name"
          name="name"
          type="text"
          autoComplete="off"
          placeholder="Add a person"
          value={name}
          onChange={event => setName(event.target.value)}
          disabled={!connected}
        />
        <button type="submit" disabled={!connected || !name.trim()}>
          Add Person
        </button>
      </form>

      <h3>People ({displayPeople.length})</h3>

      {displayPeople.length === 0 ? (
        <p className="empty-state">No people yet. Add someone to seed the table.</p>
      ) : (
        <ul className="person-list">
          {displayPeople.map((person, index) => (
            <li key={`${person.name}-${index}`} className="person-list-item">
              <span className="person-name">{person.name}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
