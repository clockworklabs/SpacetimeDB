import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import './index.css';
import { SpacetimeDBProvider } from '@clockworklabs/spacetimedb-sdk/react';
import { DbConnection } from './module_bindings/index.ts';

const connectionBuilder = DbConnection.builder()
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

    conn.subscriptionBuilder().subscribe('SELECT * FROM player');
  })
  .withToken(
    'eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMUpCQTBYRzRESFpIWUdQQk5GRFk5RDQ2SiIsImlzcyI6Imh0dHBzOi8vYXV0aC5zdGFnaW5nLnNwYWNldGltZWRiLmNvbSIsImlhdCI6MTczMDgwODUwNSwiZXhwIjoxNzkzODgwNTA1fQ.kGM4HGX0c0twL8NJoSQowzSZa8dc2Ogc-fsvaDK7otUrcdGFsZ3KsNON2eNkFh73FER0hl55_eJStr2tgoPwfTyl_v_TqkY45iUOUlLmHfB-X42cMzpE7PXbR_PKYcp-P-Wa4jGtVl4oF7CvdGKxlhIYEk3e0ElQlA9ThnZN4IEciYV0vwAXGqbaO9SOG8jbrmlmfN7oKgl02EgpodEAHTrnB2mD1qf1YyOw7_9n_EkxJxWLkJf9-nFCVRrbfSLqSJBeE6OKNAu2VLLYrSFE7GkVXNCFVugoCDM2oVJogX75AgzWimrp75QRmLsXbvB-YvvRkQ8Gfb2RZnqCj9kiYg'
  );

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </React.StrictMode>
);
