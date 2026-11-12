// @ts-nocheck -- Node driver types are not part of the model's SDK contract.
import { createInterface } from 'node:readline';
import { ClientApp } from './candidate.js';
let app;
createInterface({ input: process.stdin }).on('line', line => {
  const request = JSON.parse(line);
  try {
    switch (request.command) {
      case 'start':
        app = new ClientApp(request.uri, request.database);
        break;
      case 'add':
        app.add(request.delta);
        break;
      case 'stop':
        app.stop();
        break;
      case 'snapshot':
        break;
      default:
        throw new Error('unknown driver command');
    }
    console.log(
      'EVAL:' +
        JSON.stringify({
          id: request.id,
          ready: app.ready,
          outcome: app.outcome,
          value: app.value,
          changes: app.changes,
          identity: app.connection.identity?.toHexString(),
        })
    );
  } catch (error) {
    console.log(
      'EVAL:' + JSON.stringify({ id: request.id, error: String(error) })
    );
  }
});
