import { describe, expect, it } from 'vitest';
import { ModuleContext } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

describe('table cache policies', () => {
  it('leaves default policies implicit, including an explicit default', () => {
    for (const cachePolicy of [undefined, 'required_in_memory'] as const) {
      const ctx = new ModuleContext();
      const entry = table({ cachePolicy }, { id: t.u64() });
      ctx.moduleDef.tables.push(entry.tableDef(ctx, 'items'));
      expect(
        ctx.rawModuleDefV10().sections.find(s => s.tag === 'TableCachePolicies')
          ?.value
      ).toEqual([]);
    }
  });

  it('emits disk and object policies by table source name', () => {
    const ctx = new ModuleContext();
    for (const cachePolicy of ['required_on_disk', 'no_requirement'] as const) {
      const entry = table(
        { name: 'canonical_' + cachePolicy, cachePolicy },
        { id: t.u64() }
      );
      ctx.moduleDef.tables.push(entry.tableDef(ctx, cachePolicy));
    }
    expect(
      ctx.rawModuleDefV10().sections.find(s => s.tag === 'TableCachePolicies')
        ?.value
    ).toEqual([
      { tableName: 'required_on_disk', policy: { tag: 'RequiredOnDisk' } },
      { tableName: 'no_requirement', policy: { tag: 'NoRequirement' } },
    ]);
  });
});
