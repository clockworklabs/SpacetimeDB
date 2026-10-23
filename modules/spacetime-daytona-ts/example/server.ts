import express from 'express';
import { fileURLToPath } from 'node:url';
import { exampleUiAssetsDir } from '@spacetimedb/example-ui/server';

const app = express();
app.use('/assets', express.static(exampleUiAssetsDir));
app.use(express.static(fileURLToPath(new URL('./public/', import.meta.url))));
app.get('/api/config', (_request, response) =>
  response.json({
    uri: process.env.STDB_URI ?? 'ws://127.0.0.1:3000',
    database: process.env.SPACETIMEDB_DB_NAME ?? 'spacetime-daytona-example',
  })
);
const port = Number(process.env.PORT ?? 8816);
app.listen(port, '127.0.0.1', () =>
  console.log(`Daytona example: http://127.0.0.1:${port}`)
);
