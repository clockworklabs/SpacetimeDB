// @ts-check
import { SpacetimeDBClient } from '@clockworklabs/spacetimedb-sdk';
import { Message } from './module_bindings/message';
import { SendMessageReducer } from './module_bindings/send_message_reducer';
import { SetNameReducer } from './module_bindings/set_name_reducer';
import { User } from './module_bindings/user';

SpacetimeDBClient.registerTables(Message, User);
SpacetimeDBClient.registerReducers(SendMessageReducer, SetNameReducer);

const client = new SpacetimeDBClient('ws://localhost:3000', 'chat-node');

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
