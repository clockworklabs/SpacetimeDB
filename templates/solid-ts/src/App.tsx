import { createSignal, For, Show } from 'solid-js';
import { tables, reducers } from './module_bindings';
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/solid';

function App() {
  const [name, setName] = createSignal('');

  const conn = useSpacetimeDB();

  // Subscribe to all people in the database
  const [people] = useTable(() => tables.person);

  const addReducer = useReducer(reducers.add);

  const addPerson = (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !conn.isActive) return;

    // Call the add reducer
    addReducer({ name: name() });
    setName('');
  };

  return (
    <div style={{ padding: '2rem' }}>
      <h1>SpacetimeDB SolidJS App</h1>

      <div style={{ 'margin-bottom': '1rem' }}>
        Status:{' '}
        <strong style={{ color: conn.isActive ? 'green' : 'red' }}>
          {conn.isActive ? 'Connected' : 'Disconnected'}
        </strong>
      </div>

      <form onSubmit={addPerson} style={{ 'margin-bottom': '2rem' }}>
        <input
          type="text"
          placeholder="Enter name"
          value={name()}
          onInput={e => setName(e.currentTarget.value)}
          style={{ padding: '0.5rem', 'margin-right': '0.5rem' }}
          disabled={!conn.isActive}
        />
        <button
          type="submit"
          style={{ padding: '0.5rem 1rem' }}
          disabled={!conn.isActive}
        >
          Add Person
        </button>
      </form>

      <div>
        <h2>People ({people.length})</h2>
        <Show
          when={people.length > 0}
          fallback={<p>No people yet. Add someone above!</p>}
        >
          <ul>
            <For each={people}>
              {(person) => <li>{person.name}</li>}
            </For>
          </ul>
        </Show>
      </div>
    </div>
  );
}

export default App;
