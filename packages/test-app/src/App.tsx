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
          'c200f95df78dfd9cf328791c9fa8dcd60525d7fe361e29cf13454a6c71d91ef1'
        ),
        'eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMUpCQkNIV0pWMUpHNVg4Uk42S1E5N05GUCIsImlzcyI6Imh0dHA6Ly9sb2NhbGhvc3Q6NTE3MyIsImlhdCI6MTczMDQ2NDgxNywiZXhwIjoxNzkzNTM2ODE3fQ.aQjAg_aIa5UTr3CFTJY06_TtNLsCya_JXA3zfPlgeUm4DNXlFiTpRqnDtAtSRrArAb3WNk5LRX3XVuu23ulZzUZfq9tnHpG3ogd8-8ZmjtHB7mIAbaHUsKQs5cKRPrjvMvg6-hUdLnbLqBuMz4l2A1kl9d-XyYExXcZSl3GvvwkfoxxDAZkB7GVX557EofKCT-w8NCa3HE-1d9PEeQneVRwKh1pEKFtJcXGVAdppnp5fDTtjUKXk4uTdvWRK_psZRitwSDfE2Ikuna95c1_dtxG1MTfGQF6QyI5aHpZYnWYVtwikPZ87XiRVE41hNmQmuv9fbeG6UHNsWM7MeBA4yg',
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
