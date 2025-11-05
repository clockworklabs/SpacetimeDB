import { describe, expect, test } from 'vitest';
import { schema, table, t } from 'spacetimedb/server';

describe('idk', () => {
  test('dummy test', () => {
    const s = schema(
        table(
    {
      name: "person",
      indexes: [
        {
          name: "id_name_idx",
          algorithm: "btree",
          columns: ["age", "name"],
        },
      ],
    },
    {
      name: t.string(),
      age: t.u16(),
      rank: t.u32(),
    },
  ),

          );
    s.reducer('idk', ctx => {
      ctx.db.person.count();
    });
    expect(1 + 1).toBe(2);
  });
});
