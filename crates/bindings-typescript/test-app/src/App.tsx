import { tables, reducers } from './module_bindings';
import { useEffect } from 'react';
import './App.css';
import {
  eq,
  useReducer,
  useSpacetimeDB,
  useTable,
  where,
} from '../../src/react';

function getRandomInt(max: number) {
  return Math.floor(Math.random() * max);
}

function App() {
  const connection = useSpacetimeDB();
  const players = useTable(tables.player, where(eq('name', 'Hello')), {
    onInsert: row => {
      console.log('Player inserted:', row);
    },
  });
  const createPlayer = useReducer(reducers.createPlayer);

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
        onClick={() => {
          const player = {
            name: 'Hello',
            location: { x: getRandomInt(100), y: getRandomInt(100) },
          };
          console.log('Creating player:', player);
          createPlayer(player);
        }}
      >
        Update
      </button>
      <div>
        {Array.from(players).map((player, i) => (
          <div key={i}>
            {player.name} - ({player.location.x}, {player.location.y})
          </div>
        ))}
      </div>
    </div>
  );
}

export default App;
