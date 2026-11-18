<script lang="ts">
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/svelte';
import { tables, reducers } from './module_bindings';

const conn = useSpacetimeDB();

// Subscribe to all people in the database
const [people] = useTable(tables.person);

const addReducer = useReducer(reducers.add);

let name = $state('');
let error = $state<string | null>(null);

async function addPerson(e: SubmitEvent) {
  e.preventDefault();
  if (!name.trim() || !$conn.isActive) return;

  const newName = name;
  name = '';
  error = null;
  try {
    // Call the add reducer. It rejects if the reducer fails on the server.
    await addReducer({ name: newName });
  } catch (err) {
    error = `Couldn't add ${newName}: ${err instanceof Error ? err.message : err}`;
  }
}
</script>

<div style="padding: 2rem;">
  <h1>SpacetimeDB Svelte App</h1>

  <div style="margin-bottom: 1rem;">
    Status:
    <strong style="color: {$conn.isActive ? 'green' : 'red'}">
      {$conn.isActive ? 'Connected' : 'Disconnected'}
    </strong>
  </div>

  <form onsubmit={addPerson} style="margin-bottom: 2rem;">
    <input
      type="text"
      placeholder="Enter name"
      bind:value={name}
      style="padding: 0.5rem; margin-right: 0.5rem;"
      disabled={!$conn.isActive}
    />
    <button
      type="submit"
      style="padding: 0.5rem 1rem;"
      disabled={!$conn.isActive}
    >
      Add Person
    </button>
  </form>

  {#if error}
    <p style="color: red;">{error}</p>
  {/if}

  <div>
    <h2>People ({$people.length})</h2>
    {#if $people.length === 0}
      <p>No people yet. Add someone above!</p>
    {:else}
      <ul>
        <!-- `person` has no primary key, so key each row by its object.
             The SDK keeps the same object for a row until it changes. -->
        {#each $people as person (person)}
          <li>{person.name}</li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
