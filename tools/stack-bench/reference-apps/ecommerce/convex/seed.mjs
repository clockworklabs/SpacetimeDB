import { spawnSync } from 'node:child_process';
import { ConvexHttpClient } from 'convex/browser';
import { makeFunctionReference } from 'convex/server';
function run(name, args = {}) {
  const result = spawnSync(process.execPath, ['node_modules/convex/bin/main.js', 'run', name, JSON.stringify(args)], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`Seed failed: ${result.stderr}`);
  return result.stdout.trim() ? JSON.parse(result.stdout) : null;
}
run('seed:catalog');
const client = new ConvexHttpClient(process.env.CONVEX_SELF_HOSTED_URL);
for (const username of run('seed:missingAccounts')) {
  await client.action(makeFunctionReference('auth:signIn'), { provider: 'password',
    params: { flow: 'signUp', username, password: `stackbench-${username}-2026` } });
  run('seed:seedRole', { username });
}
