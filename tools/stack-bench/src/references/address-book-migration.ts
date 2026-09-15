import { cpSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { STACK_BENCH_ROOT } from '../package-root.js';
import { hashAppSource } from '../runtime/source-snapshot.js';
import { loadReferenceRegistry } from './reference-fixtures.js';

// Reference adapter identity for the draft exercise. This does not register a
// scored recipe; the ordinary benchmark compiler must still reject it.
export const ADDRESS_BOOK_MIGRATION_RECIPE = 'ecommerce.address-book-migration';

export function addressBookReferenceRequest(recipe: string | undefined,
  mode: string | undefined, track: string, level: number): { defect?: string } | null {
  if (!recipe || (recipe !== ADDRESS_BOOK_MIGRATION_RECIPE
    && !recipe.startsWith(`${ADDRESS_BOOK_MIGRATION_RECIPE}.`))) return null;
  const defect = recipe.slice(ADDRESS_BOOK_MIGRATION_RECIPE.length + 1);
  if (track !== 'ecommerce' || level !== 3 || !['upgrade', 'fix'].includes(mode ?? '')
    || (recipe !== ADDRESS_BOOK_MIGRATION_RECIPE && !(ADDRESS_BOOK_DEFECTS as readonly string[]).includes(defect))) {
    throw new Error('invalid address-book reference migration request');
  }
  return defect ? { defect } : {};
}

export function addressBookSchemaSource(source: string): string {
  const anchor = 'const spacetimedb = schema({';
  if (source.split(anchor).length !== 2) throw new Error('SpacetimeDB schema anchor is not unique');
  const fragment = readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/migrations/address-book/spacetime/schema.fragment'), 'utf8').replaceAll('\r\n', '\n');
  return source.replace(anchor, fragment + '\nconst spacetimedb = schema({\n  addressBookOwner,\n  addressEntry,');
}

// Apply a versioned source delta to a disposable populated reference. Never
// use the reference agent's ordinary deployment here: it resets the database.
export function applyAddressBookMigration(app: string, backend: string) {
  if (!['postgres', 'mongodb', 'spacetime'].includes(backend)) throw new Error(`address-book migration not implemented for ${backend}`);
  const baseline = loadReferenceRegistry().fixtures.find(value => value.backend === backend && value.track === 'ecommerce');
  if (!baseline || hashAppSource(app).sha256 !== baseline.imported?.sourceSha256) {
    throw new Error('address-book migration requires the exact recorded starting reference');
  }
  const httpActions = `actions={{
    load: () => addressRequest('/api/addresses'),
    save: ({ id, name, address }) => addressRequest('/api/addresses' + (id ? '/' + id : ''), id ? 'PUT' : 'POST', { name, address }),
    remove: id => addressRequest('/api/addresses/' + id, 'DELETE'),
    choose: id => addressRequest('/api/addresses/' + id + '/default', 'PUT'),
  }}`;
  const patches = backend === 'spacetime' ? [
    { file: 'backend/spacetimedb/src/index.ts', from: '  customerProfile,', to: '  customerProfile,\n  addressEntry,' },
    { file: 'backend/spacetimedb/src/index.ts', from: '    const row = { accountId: acc.id, name: name.trim(), address: address.trim() };\n    const existing = ctx.db.customerProfile.accountId.find(acc.id);\n    if (existing) ctx.db.customerProfile.accountId.update(row);\n    else ctx.db.customerProfile.insert(row);',
      to: '    saveDefaultAddress(ctx, acc.id, name, address);' },
    { file: 'client/src/components/ProgressionWorkbench.tsx', from: "import { useEffect,", to: "import { NativeAddressBook } from './NativeAddressBook';\nimport { useEffect," },
    { file: 'client/src/components/ProgressionWorkbench.tsx', from: '          <h2>Customer profile</h2>',
      to: '          <h2>Customer profile</h2>\n          {conn && <NativeAddressBook conn={conn} />}' },
  ] : backend === 'postgres' ? [
    { file: 'server/src/index.ts', from: 'import { initializeCredit,',
      to: 'import { initializeAddressBook, registerAddressBook } from "./address-book.js";\nimport { initializeCredit,' },
    { file: 'server/src/index.ts', from: 'registerProgression(app, {',
      to: 'registerAddressBook(app, pool, requireAuth, emitProgression);\nregisterProgression(app, {' },
    { file: 'server/src/index.ts', from: '  await initializeProgressionSchema(pool);',
      to: '  await initializeProgressionSchema(pool);\n  await initializeAddressBook(pool);' },
    { file: 'server/src/progression.ts', from: 'import type { Request,',
      to: 'import { saveDefaultAddress } from "./address-book.js";\nimport type { Request,' },
    { file: 'server/src/progression.ts', from: '    const name = String(req.body?.name ?? "").trim();\n    const address = String(req.body?.address ?? "").trim();\n    await deps.pool.query(`UPDATE account SET profile_name = $1, profile_address = $2 WHERE id = $3`,\n      [name, address, req.account!.id]);',
      to: '    const name = req.body?.name;\n    const address = req.body?.address;\n    await saveDefaultAddress(deps.pool, req.account!.id, name, address);' },
    { file: 'client/src/ProgressionPanel.tsx', from: 'import { request }',
      to: 'import { AddressBook } from "./AddressBook";\nimport { request }' },
    { file: 'client/src/ProgressionPanel.tsx', from: '        <h3>Profile</h3>',
      to: `        <h3>Profile</h3>\n        <AddressBook ${httpActions.replaceAll('addressRequest', 'request')} />` },
  ] : [
    { file: 'server/src/index.ts', from: 'import { installProgressionRoutes }',
      to: 'import { initializeAddressBook } from "./address-book.js";\nimport { installProgressionRoutes }' },
    { file: 'server/src/index.ts', from: '  await seed();', to: '  await seed();\n  await initializeAddressBook();' },
    { file: 'server/src/progression.ts', from: 'import type express',
      to: 'import { registerAddressBook, saveDefaultAddress } from "./address-book.js";\nimport type express' },
    { file: 'server/src/progression.ts', from: '  async function recordActivity(user:',
      to: '  registerAddressBook(app, auth, changed);\n\n  async function recordActivity(user:' },
    { file: 'server/src/progression.ts', from: '    const name = cleanText(req.body?.name);\n    const address = cleanText(req.body?.address);\n    if (!name || !address) return res.status(400).json({ error: "Name and address are required" });\n    const profile = await Profile.findOneAndUpdate({ userId: req.progressionUser._id },\n      { userId: req.progressionUser._id, name, address }, { upsert: true, new: true });',
      to: '    await saveDefaultAddress(req.progressionUser._id, req.body?.name, req.body?.address);\n    const profile = await Profile.findOne({ userId: req.progressionUser._id });' },
    { file: 'client/src/ProgressionPanel.tsx', from: 'import { request }',
      to: 'import { AddressBook } from "./AddressBook";\nimport { request }' },
    { file: 'client/src/ProgressionPanel.tsx', from: '      <h3>Profile</h3>',
      to: `      <h3>Profile</h3>\n      <AddressBook ${httpActions} />` },
    { file: 'client/src/ProgressionPanel.tsx', from: '  const saveProfile = () =>',
      to: '  const addressRequest = (path: string, method = "GET", body?: unknown) => request(path, token, { method, body: body === undefined ? undefined : JSON.stringify(body) });\n  const saveProfile = () =>' },
    { file: 'client/src/ProgressionPanel.tsx',
      from: '      if (next.profile) {\n        setProfileName(next.profile.name || "");\n        setProfileAddress(next.profile.address || "");\n      }', to: '' },
    { file: 'client/src/ProgressionPanel.tsx', from: '  const act = async (path:',
      to: '  useEffect(() => {\n    setProfileName(state.profile?.name ?? "");\n    setProfileAddress(state.profile?.address ?? "");\n  }, [state.profile?.name, state.profile?.address, token]);\n\n  const act = async (path:' },
  ];
  // Check every anchor before writing any source file.
  const changed = new Map<string, string>();
  for (const patch of patches) {
    const source = changed.get(patch.file) ?? readFileSync(join(app, patch.file), 'utf8').replaceAll('\r\n', '\n');
    if (source.split(patch.from).length !== 2) throw new Error(`migration anchor is not unique: ${patch.file}`);
    changed.set(patch.file, source.replace(patch.from, patch.to));
  }
  if (backend === 'spacetime') {
    const schemaFile = 'backend/spacetimedb/src/schema.ts';
    changed.set(schemaFile, addressBookSchemaSource(readFileSync(join(app, schemaFile), 'utf8').replaceAll('\r\n', '\n')));
    const file = 'backend/spacetimedb/src/index.ts';
    changed.set(file, changed.get(file)! + readFileSync(join(STACK_BENCH_ROOT,
      'reference-apps/migrations/address-book/spacetime/reducers.fragment'), 'utf8'));
    cpSync(join(STACK_BENCH_ROOT, 'reference-apps/migrations/address-book/spacetime/client'), join(app, 'client'), { recursive: true });
  } else cpSync(join(STACK_BENCH_ROOT, 'reference-apps/migrations/address-book', backend), app, { recursive: true });
  cpSync(join(STACK_BENCH_ROOT, 'reference-apps/migrations/address-book/shared/AddressBook.tsx'), join(app, 'client/src/AddressBook.tsx'));
  for (const [file, contents] of changed) writeFileSync(join(app, file), contents);
  return hashAppSource(app);
}

export const ADDRESS_BOOK_DEFECTS = ['no-import', 'wrong-owner', 'changed-history', 'stale-profile', 'resurrect', 'cross-account'] as const;
export function applyAddressBookDefect(app: string, backend: string, defect: string) {
  if (!['postgres', 'mongodb', 'spacetime'].includes(backend)) throw new Error(`unsupported defect backend: ${backend}`);
  if (!(ADDRESS_BOOK_DEFECTS as readonly string[]).includes(defect)) throw new Error(`unknown address-book defect: ${defect}`);
  const path = join(app, backend === 'spacetime' ? 'backend/spacetimedb/src/index.ts' : 'server/src/address-book.ts');
  let source = readFileSync(path, 'utf8').replaceAll('\r\n', '\n');
  const change = (from: string, to: string) => {
    if (source.split(from).length !== 2) throw new Error(`address-book defect anchor is not unique: ${defect}`);
    source = source.replace(from, to);
  };
  if (backend === 'spacetime') switch (defect) {
    case 'no-import':
      change("if (old && (old.name !== '' || old.address !== '')) {", "if (old && old.name === 'Never import this fixture') {"); break;
    case 'wrong-owner':
      change('const old = ctx.db.customerProfile.accountId.find(accountId);',
        `const other = [...ctx.db.account.iter()].find(a => a.username === (ctx.db.account.id.find(accountId)?.username === 'm8-customer' ? 'admin-helper' : 'm8-customer'));
  const old = ctx.db.customerProfile.accountId.find(other?.id ?? accountId);`); break;
    case 'changed-history':
      change('ctx.db.addressBookOwner.insert({ accountId });',
        'ctx.db.addressBookOwner.insert({ accountId });\n  const line = ctx.db.orderItem.iter().next().value;\n  if (line) ctx.db.orderItem.id.update({ ...line, unitPrice: line.unitPrice + 0.01 });'); break;
    case 'stale-profile':
      change('function syncAddressProfile(ctx: Ctx, accountId: bigint) {', 'function syncAddressProfile(ctx: Ctx, accountId: bigint) {\n  return;'); break;
    case 'resurrect':
      change('if (ctx.db.addressBookOwner.accountId.find(accountId)) return;',
        'if ([...ctx.db.addressEntry.accountId.filter(accountId)].length) return;');
      change('ctx.db.addressBookOwner.insert({ accountId });',
        'if (!ctx.db.addressBookOwner.accountId.find(accountId)) ctx.db.addressBookOwner.insert({ accountId });');
      change("name: entry?.name ?? '', address: entry?.address ?? ''", "name: entry?.name ?? 'Restored', address: entry?.address ?? 'Resurrected deleted address'"); break;
    case 'cross-account':
      change("if (!entry || entry.accountId !== accountId) throw new SenderError('Address missing');", "if (!entry) throw new SenderError('Address missing');"); break;
  }
  else if (backend === 'mongodb') switch (defect) {
    case 'no-import':
      change("const entries = profile && (profile.name !== '' || profile.address !== '')", 'const entries = profile && false'); break;
    case 'wrong-owner':
      change('const profile = await Profile.findOne({ userId }).lean();',
        `const owner = await User.findById(userId);
  const other = await User.findOne({ username: owner?.username === 'm8-customer' ? 'admin-helper' : 'm8-customer' });
  const profile = await Profile.findOne({ userId: other?._id ?? userId }).lean();`); break;
    case 'changed-history':
      change('await Book.init();', "await Book.init();\n  await mongoose.connection.collection('orders').updateOne({}, { $inc: { 'items.0.price': 0.01 } });"); break;
    case 'stale-profile':
      change('await Profile.updateOne({ userId },', 'if (false) await Profile.updateOne({ userId },'); break;
    case 'resurrect':
      change('const profile = await Profile.findOne({ userId }).lean();',
        'await Book.deleteOne({ _id: userId, entries: { $size: 0 } });\n  const profile = await Profile.findOne({ userId }).lean();');
      change("name: selected?.name ?? '', address: selected?.address ?? ''", "name: selected?.name ?? 'Restored', address: selected?.address ?? 'Resurrected deleted address'"); break;
    case 'cross-account':
      change("const result = await withBook(req.progressionUser._id, method !== 'get',",
        "const owner = req.params.id ? await Book.findOne({ 'entries._id': req.params.id }) : null;\n        const result = await withBook(owner?._id ?? req.progressionUser._id, method !== 'get',"); break;
  }
  else switch (defect) {
    case 'no-import':
      change("WHERE a.profile_name <> '' OR a.profile_address <> '';", 'WHERE false;');
      break;
    case 'wrong-owner':
      change('SELECT a.id, a.profile_name, a.profile_address, true FROM account a',
        `SELECT CASE WHEN a.username = 'm8-customer' THEN (SELECT id FROM account WHERE username='admin-helper')
          WHEN a.username = 'admin-helper' THEN (SELECT id FROM account WHERE username='m8-customer')
          ELSE a.id END, a.profile_name, a.profile_address, true FROM account a`);
      break;
    case 'changed-history':
      change("WHERE a.profile_name <> '' OR a.profile_address <> '';",
        "WHERE a.profile_name <> '' OR a.profile_address <> '';\nUPDATE order_item SET price=price+0.01 WHERE id=(SELECT MIN(id) FROM order_item);");
      break;
    case 'stale-profile':
      change('async function syncProfile(client: PoolClient, accountId: number) {',
        'async function syncProfile(client: PoolClient, accountId: number) {\n  return;');
      break;
    case 'resurrect':
      change("    if (imported.rows.length && (account.rows[0].profile_name !== '' || account.rows[0].profile_address !== '')) {",
        "    if (!(await client.query('SELECT 1 FROM address_entry WHERE account_id=$1', [accountId])).rows.length\n      && (account.rows[0].profile_name !== '' || account.rows[0].profile_address !== '')) {");
      change("[accountId, result.rows[0]?.name ?? '', result.rows[0]?.address ?? '']);",
        "[accountId, result.rows[0]?.name ?? 'Restored', result.rows[0]?.address ?? 'Resurrected deleted address']);");
      break;
    case 'cross-account':
      change('const result = await withBook(pool, req.account!.id,',
        "const owner = req.params.id ? await pool.query('SELECT account_id FROM address_entry WHERE id::text=$1', [req.params.id]) : null;\n        const ownerId = owner?.rows[0]?.account_id ?? req.account!.id;\n        const result = await withBook(pool, ownerId,");
      change("client => work(client, req.account!.id, String(req.params.id ?? ''), req.body)", "client => work(client, ownerId, String(req.params.id ?? ''), req.body)");
      break;
  }
  writeFileSync(path, source);
  return hashAppSource(app);
}
