
import { describe, expect, test } from 'vitest';
import { schema, table, t } from "spacetimedb/server";


describe('idk', () => {
  test('dummy test', () => {
    const s = schema(
        table(
            { name: "person" },
            {
            name: t.string(),
            },
        ),
        );
        s.reducer("idk", (ctx) => {
            ctx.db.person.count();
        });
    expect(1 + 1).toBe(2);
  });
});