import { DbConnection, Player } from './module_bindings';
import { useEffect } from 'react';
import './App.css';
import { useSpacetimeDB, useTable } from '@clockworklabs/spacetimedb-sdk/react';

function App() {
  const connection = useSpacetimeDB<DbConnection>();
  const players = useTable<DbConnection, Player>('player', {
    onInsert: player => {
      console.log(player);
    },
  });

  useEffect(() => {
    setTimeout(() => {
      console.log(Array.from(players.rows));
    }, 5000);
  }, [connection, players.rows]);

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
