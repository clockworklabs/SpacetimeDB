import { generateKeyPairSync } from 'node:crypto';
import { spawnSync } from 'node:child_process';
function cli(args) {
  const result = spawnSync(process.execPath, ['node_modules/convex/bin/main.js', ...args], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`Convex configuration failed: ${result.stderr}`);
  return result.stdout.trim();
}
const privateKey = cli(['env', 'get', 'JWT_PRIVATE_KEY']);
const jwks = cli(['env', 'get', 'JWKS']);
if (Boolean(privateKey) !== Boolean(jwks)) throw new Error('Incomplete existing signing key configuration');
if (!privateKey) {
  const keys = generateKeyPairSync('rsa', { modulusLength: 2048 });
  cli(['env', 'set', 'JWT_PRIVATE_KEY', '--', keys.privateKey.export({ type: 'pkcs8', format: 'pem' }).trimEnd()]);
  cli(['env', 'set', 'JWKS', '--', JSON.stringify({ keys: [{ ...keys.publicKey.export({ format: 'jwk' }), use: 'sig' }] })]);
}
