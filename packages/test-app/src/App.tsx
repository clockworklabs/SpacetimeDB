import { DBConnection } from './module_bindings';
import { useState } from 'react';
import './App.css';
import { Identity } from '@clockworklabs/spacetimedb-sdk';

function App() {
  const [connection] = useState<DBConnection>(
    DBConnection.builder()
      .withUri('ws://localhost:3000')
      .withModuleName('goldbreezycanid')
      .onDisconnect(() => {
        console.log('disconnected');
      })
      .onConnectError(() => {
        console.log('client_error');
      })
      .onConnect((_, identity, _token) => {
        console.log(
          'Connected to SpacetimeDB with identity:',
          identity.toHexString()
        );
      })
      .withCredentials([
        Identity.fromString(
          '49f2d472cabfbc7ded52ac1f93316750dc8ea162aac97cc52a340aed221b7ff3'
        ),
        'eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI0OWYyZDQ3MmNhYmZiYzdkZWQ1MmFjMWY5MzMxNjc1MGRjOGVhMTYyYWFjOTdjYzUyYTM0MGFlZDIyMWI3ZmYzIiwiaWF0IjoxNjgwMTkwNDc5fQ.KPz0DjrWb6I5c51wa71FGTgWz0Nh6CiNycM0ynmDDNkGjRxsci5cmiEjHQdYKyIeaG9MizSVPGlaDJ2Z7uctcg',
      ])
      .build()
  );

  return (
    <div className="App">
      <h1>Typescript SDK Test!</h1>
      <p>{connection.identity?.toHexString()}</p>
    </div>
  );
}

export default App;
