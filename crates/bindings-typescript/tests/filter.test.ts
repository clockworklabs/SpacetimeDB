import { describe, expect, it } from 'vitest';
import { eq, evaluate, toString } from '../src/lib/filter';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { Timestamp } from '../src/lib/timestamp';
import { t } from '../src/lib/type_builders';

const peopleTableDef = table(
  { name: 'people' },
  {
    createdAt: t.timestamp(),
    id: t.u32(),
  }
);

const schemaDef = tablesToSchema(new ModuleContext(), {
  people: peopleTableDef,
});

describe('filter.ts timestamp support', () => {
  it('evaluates timestamp equality by microseconds', () => {
    const ts = Timestamp.fromDate(new Date('2024-01-01T00:00:00.123Z'));

    expect(evaluate(eq('createdAt', ts), { createdAt: ts })).toBe(true);
    expect(
      evaluate(eq('createdAt', ts), {
        createdAt: new Timestamp(ts.microsSinceUnixEpoch + 1n),
      })
    ).toBe(false);
  });

  it('renders timestamp literals as ISO strings', () => {
    const ts = Timestamp.fromDate(new Date('2024-01-01T00:00:00.123Z'));

    expect(toString(schemaDef.tables.people, eq('createdAt', ts))).toBe(
      `createdAt = '2024-01-01T00:00:00.123000Z'`
    );
  });
});
