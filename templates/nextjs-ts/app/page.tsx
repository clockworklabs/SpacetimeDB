import { PersonList } from './PersonList';
import { fetchPeople } from '../lib/spacetimedb-server';

export default async function Home() {
  // Fetch initial data on the server
  let initialPeople: Awaited<ReturnType<typeof fetchPeople>> = [];

  try {
    initialPeople = await fetchPeople();
  } catch (error) {
    // If server-side fetch fails, the client will still work
    // This can happen if the database is not yet published
    console.error('Failed to fetch initial data:', error);
  }

  return (
    <main style={{ padding: '2rem', fontFamily: 'system-ui, sans-serif' }}>
      <h1>SpacetimeDB Next.js App</h1>
      <PersonList initialPeople={initialPeople} />
    </main>
  );
}
