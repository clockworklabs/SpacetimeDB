// @ts-check
import { SpacetimeDBClient } from '@clockworklabs/spacetimedb-sdk';
import { Message } from './module_bindings/message';
import { SendMessageReducer } from './module_bindings/send_message_reducer';
import { SetNameReducer } from './module_bindings/set_name_reducer';
import { User } from './module_bindings/user';

SpacetimeDBClient.registerTables(Message, User);
SpacetimeDBClient.registerReducers(SendMessageReducer, SetNameReducer);

const client = new SpacetimeDBClient(
  'ws://localhost:3000',
  'chat-node',
  'eyJ0eXAiOiJKV1QiLCJhbGciOiJFUzI1NiJ9.eyJoZXhfaWRlbnRpdHkiOiI3YmEwZWNmOThkNmQ1NzVkZTQ4NTBiZWE0ZmE0MGFmYTE5YmNmMjVjNmQzNmFlMWE0M2M2NGUyMGJlNjIxMTU5IiwiaWF0IjoxNzIzMDE0MjUwLCJleHAiOm51bGx9.aZhU1LL3peqhNih_eDQp-VrgcKvuODZt0nBieV1HAPy82mvAFVDJwaBKWY6zBIFrzORFlcLK9mPjIphhfSmoEA'
);

client.connect();
client.on('disconnected', () => {
  console.log('disconnected');
});
client.on('client_error', () => {
  console.log('client_error');
});

client.on('connected', e => {
  // logs the identity
  console.log(e);
});
