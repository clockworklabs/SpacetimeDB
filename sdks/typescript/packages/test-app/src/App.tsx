import { DBConnection } from './module_bindings';
import { useEffect, useState } from 'react';
import './App.css';

function App() {
  const [connection] = useState(() =>
    DBConnection.builder()
      .withUri('ws://localhost:3000')
      .withModuleName('game')
      .withLightMode(true)
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
      .withToken(
        'eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMUpCQTBYRzRESFpIWUdQQk5GRFk5RDQ2SiIsImlzcyI6Imh0dHBzOi8vYXV0aC5zdGFnaW5nLnNwYWNldGltZWRiLmNvbSIsImlhdCI6MTczMDgwODUwNSwiZXhwIjoxNzkzODgwNTA1fQ.kGM4HGX0c0twL8NJoSQowzSZa8dc2Ogc-fsvaDK7otUrcdGFsZ3KsNON2eNkFh73FER0hl55_eJStr2tgoPwfTyl_v_TqkY45iUOUlLmHfB-X42cMzpE7PXbR_PKYcp-P-Wa4jGtVl4oF7CvdGKxlhIYEk3e0ElQlA9ThnZN4IEciYV0vwAXGqbaO9SOG8jbrmlmfN7oKgl02EgpodEAHTrnB2mD1qf1YyOw7_9n_EkxJxWLkJf9-nFCVRrbfSLqSJBeE6OKNAu2VLLYrSFE7GkVXNCFVugoCDM2oVJogX75AgzWimrp75QRmLsXbvB-YvvRkQ8Gfb2RZnqCj9kiYg'
      )
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
