import { schema, table, t } from '../../../sdks/typescript/src/server';

const spacetime = schema(
  table(
    {
      name: 'person',
      public: true,
      indexes: [{ name: 'age', algorithm: 'btree', columns: ['age'] }],
    },
    {
      id: t.u32().primaryKey().autoInc(),
      name: t.string(),
      age: t.u8(),
    }
  )
);

spacetime.reducer(
  'add',
  { name: t.string(), age: t.u8() },
  (ctx, { name, age }) => {
    ctx.db.person.insert({ id: 0, name, age });
  }
);

spacetime.reducer('say_hello', {}, ctx => {
  for (const person of ctx.db.person.iter()) {
    console.log(`Hello, ${person.name}}!`);
  }
  console.log('Hello, World!');
});
