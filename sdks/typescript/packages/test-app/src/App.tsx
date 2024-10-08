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
        'eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI5M2RkYTA5ZGI5YTU2ZDhmYTZjMDI0ZDg0M2U4MDVkODI2MjE5MWRiM2I0YmE4NGM1ZWZjZDFhZDQ1MWZlZDRlIiwiaWF0IjoxNzI4MzY3NDk0LCJleHAiOm51bGx9.ua3DQaiXo5i3Ye0okughs84PV9uZLCnov26zvCMG-3ibqGg6vH0uDDY6L-zDf8XiFgUqEIcvnJvuKLThPDipjQ',
      ])
      .build()
  );

  useEffect(() => {
    connection.db.player.onInsert(player => {
      console.log(player);
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
