import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import './index.css';
import { SpacetimeDBProvider } from '../../src/react';
import { DbConnection, query } from './module_bindings/index.ts';

const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('game')
  .withLightMode(true)
  .onDisconnect(() => {
    console.log('disconnected');
  })
  .onConnectError((ctx, err) => {
    console.log('client_error: ', err);
  })
  .onConnect((conn, identity, _token) => {
    console.log(
      'Connected to SpacetimeDB with identity:',
      identity.toHexString()
    );

    conn.subscriptionBuilder().subscribe(query.player.build());
  });

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </React.StrictMode>
);
