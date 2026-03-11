<template>
  <div :style="{ padding: '2rem' }">
    <h1>SpacetimeDB Nuxt App</h1>

    <div :style="{ marginBottom: '1rem' }">
      Status:
      <strong :style="{ color: conn?.isActive ? 'green' : 'red' }">
        {{ conn?.isActive ? 'Connected' : 'Disconnected' }}
      </strong>
    </div>

    <form @submit.prevent="addPerson" :style="{ marginBottom: '2rem' }">
      <input
        type="text"
        placeholder="Enter name"
        v-model="name"
        :style="{ padding: '0.5rem', marginRight: '0.5rem' }"
        :disabled="!conn?.isActive"
      />
      <button
        type="submit"
        :style="{ padding: '0.5rem 1rem' }"
        :disabled="!conn?.isActive"
      >
        Add Person
      </button>
    </form>

    <div>
      <h2>People ({{ displayPeople.length }})</h2>
      <p v-if="displayPeople.length === 0">No people yet. Add someone above!</p>
      <ul v-else>
        <li v-for="(person, index) in displayPeople" :key="index">
          {{ person.name }}
        </li>
      </ul>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { tables, reducers } from '../module_bindings';

const name = ref('');

// Fetch initial data server-side for SSR
const { data: initialPeople } = await useFetch('/api/people');

// On the client, use real-time composables
let conn:
  | ReturnType<typeof import('spacetimedb/vue').useSpacetimeDB>
  | undefined;
let people:
  | ReturnType<typeof import('spacetimedb/vue').useTable>[0]
  | undefined;
let isReady:
  | ReturnType<typeof import('spacetimedb/vue').useTable>[1]
  | undefined;
let addReducer:
  | ReturnType<typeof import('spacetimedb/vue').useReducer>
  | undefined;

if (import.meta.client) {
  try {
    const { useSpacetimeDB, useTable, useReducer } = await import(
      'spacetimedb/vue'
    );
    conn = useSpacetimeDB();
    const [tableData, tableReady] = useTable(tables.person);
    people = tableData;
    isReady = tableReady;
    addReducer = useReducer(reducers.add);
  } catch {}
}

// Use real-time data once connected, fall back to SSR data
const displayPeople = computed(() => {
  if (conn?.isActive && people?.value) {
    return people.value;
  }
  return initialPeople.value ?? [];
});

const addPerson = () => {
  if (!name.value.trim() || !conn?.isActive || !addReducer) return;
  addReducer({ name: name.value });
  name.value = '';
};
</script>
