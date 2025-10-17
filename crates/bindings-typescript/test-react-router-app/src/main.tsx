import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import './index.css';
import { SpacetimeDBProvider } from '../../src/react';
import { DbConnection, ErrorContext } from './module_bindings/index.ts';
import { BrowserRouter } from 'react-router-dom';
import { Identity } from '../../src/index.ts';

const onConnect = (_conn: DbConnection, identity: Identity, token: string) => {
  localStorage.setItem('stdbToken', token);
  console.log('Connected to SpacetimeDB! [' + identity.toHexString() + ']');
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB!');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.log('Error connecting to SpacetimeDB! ', err);
};

// we DO NOT .build() the builder here, that's done automatically in the <SpacetimeDBProvider> component
// set all the settings you need, be sure your Uri and Module Name are correct
const connBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('simple-stdb-react-hooks-example')
  .withToken(localStorage.getItem('stdbToken') || '')
  .onConnect(onConnect)
  .onConnectError(onConnectError)
  .onDisconnect(onDisconnect);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SpacetimeDBProvider connectionBuilder={connBuilder}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </SpacetimeDBProvider>
  </React.StrictMode>
);
