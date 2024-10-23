import { DBConnection } from './module_bindings';
import { useEffect, useState } from 'react';
import './App.css';
import { Identity } from '@clockworklabs/spacetimedb-sdk';

function App() {
  const [connection] = useState(() =>
    DBConnection.builder()
      .withUri('ws://localhost:3000')
      .withModuleName('game')
      .onDisconnect(() => {
        console.log('disconnected');
      })
      .onConnectError(() => {
        console.log('client_error');
      })
      .onConnect((conn, identity, _token) => {
        console.log(
          'Connected to SpacetimeDB with identity:',
          identity.toHexString()
        );

        conn.subscriptionBuilder().subscribe(['SELECT * FROM player']);
      })
      .withCredentials([
        Identity.fromString(
          '93dda09db9a56d8fa6c024d843e805d8262191db3b4ba84c5efcd1ad451fed4e'
        ),
        'eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI5M2RkYTA5ZGI5YTU2ZDhmYTZjMDI0ZDg0M2U4MDVkODI2MjE5MWRiM2I0YmE4NGM1ZWZjZDFhZDQ1MWZlZDRlIiwiaWF0IjoxNzI4Mzc5MjE2LCJleHAiOm51bGx9.dKanuJu7xKg_g3toOBO09Po3ZgxnHnUwZYpwbEwjrHWGkRzNSL9sLRNjKatUR7OXmwd9b0pCTray4GUt0VlCGg',
      ])
      .build()
  );

  useEffect(() => {
    connection.db.player.onInsert((ctx, player) => {
      console.log(ctx, player);
    });

    setTimeout(() => {
      console.log(Array.from(connection.db.player.iter()));
    }, 5000);
  }, [connection]);

  return (
    <div className="App">
      <h1>Typescript SDK Test!</h1>
      <p>{connection.identity?.toHexString()}</p>

      <button
        onClick={() =>
          connection.reducers.createPlayer('Hello', { x: 10, y: 40 })
        }
      >
        Update
      </button>
    </div>
  );
}

export default App;
