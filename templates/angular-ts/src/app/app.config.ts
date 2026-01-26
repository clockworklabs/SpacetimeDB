import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter } from '@angular/router';

import { routes } from './app.routes';
import { provideSpacetimeDB } from 'spacetimedb/angular';
import { DbConnection } from '../module_bindings';

const builder = DbConnection.builder()
  .withUri('http://localhost:4000')
  .withModuleName('angular-ts')
  .onConnect((_, identity, token) =>
    console.log('Connected to SpacetimeDB as ' + identity + ' with token ' + token),
  )
  .onConnectError((_, err) => console.error('Connection error', err))
  .onDisconnect((_, err) => console.error('Disconnected from SpacetimeDB ' + err));

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    provideSpacetimeDB(builder),
  ],
};
