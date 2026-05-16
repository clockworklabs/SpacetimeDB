/* @refresh reload */
import './index.css';

import { render, Suspense } from 'solid-js/web';

import App from './app';
import { Router } from '@solidjs/router';
import { routes } from './routes';
import { SpacetimeDBProvider } from '../../src/solid';
import { DbConnection, ErrorContext } from './module_bindings/index.ts';
import { Identity } from '../../src/index.ts';

const root = document.getElementById('root');

if (import.meta.env.DEV && !(root instanceof HTMLElement)) {
  throw new Error(
    'Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?'
  );
}

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
  .withDatabaseName('simple-stdb-solid-hooks-example')
  .withToken(localStorage.getItem('stdbToken') || '')
  .onConnect(onConnect)
  .onConnectError(onConnectError)
  .onDisconnect(onDisconnect);

render(
  () => (
    <SpacetimeDBProvider connectionBuilder={connBuilder}>
      <Router root={props => <App>{props.children}</App>}>{routes}</Router>
    </SpacetimeDBProvider>
  ),
  root!
);
