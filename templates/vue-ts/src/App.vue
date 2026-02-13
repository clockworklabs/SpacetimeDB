<template>
  <div :style="{ padding: '2rem' }">
    <h1>SpacetimeDB Vue App</h1>

    <div :style="{ marginBottom: '1rem' }">
      Status:
      <strong :style="{ color: conn.isActive ? 'green' : 'red' }">
        {{ conn.isActive ? 'Connected' : 'Disconnected' }}
      </strong>
    </div>

    <form @submit.prevent="addPerson" :style="{ marginBottom: '2rem' }">
      <input
        type="text"
        placeholder="Enter name"
        v-model="name"
        :style="{ padding: '0.5rem', marginRight: '0.5rem' }"
        :disabled="!conn.isActive"
      />
      <button
        type="submit"
        :style="{ padding: '0.5rem 1rem' }"
        :disabled="!conn.isActive"
      >
        Add Person
      </button>
    </form>

    <div>
      <h2>People ({{ people.length }})</h2>
      <p v-if="people.length === 0">No people yet. Add someone above!</p>
      <ul v-else>
        <li v-for="(person, index) in people" :key="index">
          {{ person.name }}
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { tables, reducers } from './module_bindings';
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/vue';

const conn = useSpacetimeDB();
const name = ref('');

// Subscribe to all people in the database
const [people] = useTable(tables.person);

const addReducer = useReducer(reducers.add);

const addPerson = () => {
  if (!name.value.trim() || !conn.isActive) return;

  // Call the add reducer
  addReducer({ name: name.value });
  name.value = '';
};
</script>
