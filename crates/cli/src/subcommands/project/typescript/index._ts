import { schema, table, t } from 'spacetimedb/server';

export const spacetimedb = schema(
  table(
    { name: 'person' },
    {
      name: t.string(),
    }
  )
);

spacetimedb.reducer('init', (_ctx) => {
  // Called when the module is initially published
});

spacetimedb.reducer('client_connected', (_ctx) => {
  // Called every time a new client connects
});

spacetimedb.reducer('client_disconnected', (_ctx) => {
  // Called every time a client disconnects
});

spacetimedb.reducer('add', { name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ name });
});

spacetimedb.reducer('say_hello', (ctx) => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});