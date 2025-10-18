import { useState } from 'react';
import { DbConnection, Person } from './module_bindings';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';

function App() {
  const [name, setName] = useState('');

  const conn = useSpacetimeDB<DbConnection>();
  const { isActive: connected } = conn;

  // Subscribe to all people in the database
  const { rows: people } = useTable<DbConnection, Person>('person');

  const addPerson = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !connected) return;

    // Call the add reducer
    conn.reducers.add(name);
    setName('');
  };

  return (
    <div style={{ padding: '2rem' }}>
      <h1>SpacetimeDB React App</h1>

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
    </div>
  );
}

export default App;
