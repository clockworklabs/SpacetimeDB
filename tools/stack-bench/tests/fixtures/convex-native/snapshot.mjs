import assert from 'node:assert/strict';

// Fixture reader through the public Data Sync API, never generated app code.
// Retain raw pages because native timestamp numbers exceed JS integer precision.
export async function snapshot(url, adminKey) {
  const selection = { _other: 'excluded', '': { _other: 'excluded',
    items: { _other: 'included' }, orders: { _other: 'included' } } };
  const tables = new Map(); const pages = []; let cursor;
  for (let page = 0; page < 30; page++) {
    const response = await fetch(`${url}/api/v1/data/sync`, { method: 'POST',
      headers: { Authorization: `Convex ${adminKey}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ selection, ...(cursor ? { cursor } : {}) }), signal: AbortSignal.timeout(10000) });
    const raw = await response.text(); pages.push({ httpStatus: response.status, raw });
    assert.equal(response.status, 200, raw);
    const result = JSON.parse(raw);
    for (const row of result.truncates) {
      assert.equal(row.component, '');
      tables.set(row.table, new Map());
    }
    for (const row of result.values) {
      assert.equal(row.component, '');
      assert(tables.has(row.table), 'Snapshot must declare the table before its values');
      assert.equal(typeof row.value._id, 'string');
      if (row.deleted) tables.get(row.table).delete(row.value._id);
      else tables.get(row.table).set(row.value._id, row.value);
    }
    if (result.status.type === 'upToDate') {
      const snapshotTs = /"snapshotTs"\s*:\s*(\d+)/.exec(raw)?.[1];
      assert(snapshotTs, 'Require exact snapshot timestamp');
      assert(tables.has('items') && tables.has('orders'), 'Snapshot must include both tables');
      return { snapshotTs, pages, tables: Object.fromEntries([...tables].map(([key, rows]) =>
        [key, [...rows.values()].sort((a, b) => a._id.localeCompare(b._id))])) };
    }
    assert(['snapshotting', 'stale'].includes(result.status.type), 'Unknown consistency state');
    assert(result.pagination.nextCursor && result.pagination.nextCursor !== cursor, 'Cursor must advance');
    cursor = result.pagination.nextCursor;
  }
  throw new Error('Fixture snapshot did not complete within 30 pages');
}
