<script lang="ts">
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/svelte';
import { tables, reducers } from './module_bindings';

const conn = useSpacetimeDB();

// Subscribe to all people in the database
const [people] = useTable(tables.person);

const addReducer = useReducer(reducers.add);

let name = $state('');

function addPerson(e: SubmitEvent) {
  e.preventDefault();
  if (!name.trim() || !$conn.isActive) return;

  // Call the add reducer
  addReducer({ name: name });
  name = '';
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

  <div>
    <h2>People ({$people.length})</h2>
    {#if $people.length === 0}
      <p>No people yet. Add someone above!</p>
    {:else}
      <ul>
        {#each $people as person}
          <li>{person.name}</li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
