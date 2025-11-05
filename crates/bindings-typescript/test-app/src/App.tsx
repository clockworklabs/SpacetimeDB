import { tables, reducers, DbConnection } from './module_bindings';
import { useEffect } from 'react';
import './App.css';
import { useReducer, useSpacetimeDB, useTable } from '../../src/react';

function App() {
  const connection = useSpacetimeDB();
  const players = useTable(tables.player);
  const createPlayer = useReducer(reducers.createPlayer);
  createPlayer({ name: 'Test', location: { x: 0, y: 0 } });

  useEffect(() => {
    setTimeout(() => {
      console.log(Array.from(players));
    }, 5000);
  }, [connection, players]);

  return (
    <div className="App">
      <h1>Typescript SDK Test!</h1>
      <p>{connection.identity?.toHexString()}</p>

      <button
        onClick={() =>
          connection.getConnection<DbConnection>()?.reducers.createPlayer({
            name: 'Hello',
            location: { x: 10, y: 40 },
          })
        }
      >
        Update
      </button>
    </div>
  );
}

export default App;
