import { SpacetimeDBClient } from '@clockworklabs/spacetimedb-sdk';
import { useEffect, useState } from 'react';
import './App.css';

function App() {
  const [client] = useState<SpacetimeDBClient>(
    new SpacetimeDBClient(
      'localhost:3000',
      'goldbreezycanid',
      // @ts-ignore
      // TODO: WHy are these not according to the types?
      {
        identity:
          '49f2d472cabfbc7ded52ac1f93316750dc8ea162aac97cc52a340aed221b7ff3',
        token:
          'eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI0OWYyZDQ3MmNhYmZiYzdkZWQ1MmFjMWY5MzMxNjc1MGRjOGVhMTYyYWFjOTdjYzUyYTM0MGFlZDIyMWI3ZmYzIiwiaWF0IjoxNjgwMTkwNDc5fQ.KPz0DjrWb6I5c51wa71FGTgWz0Nh6CiNycM0ynmDDNkGjRxsci5cmiEjHQdYKyIeaG9MizSVPGlaDJ2Z7uctcg',
      }
    )
  );

  useEffect(() => {
    client.connect();
    client.on('disconnected', () => {
      console.log('disconnected');
    });
    client.on('client_error', () => {
      console.log('client_error');
    });

    client.on('connected', e => {
      // logs the identity
      console.log(e);
    });
  }, [client]);

  return (
    <div className="App">
      <h1>Typescript SDK Test!</h1>
    </div>
  );
}

export default App;
