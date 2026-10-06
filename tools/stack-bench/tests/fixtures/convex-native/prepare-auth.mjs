import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
for (const [name, value] of Object.entries({
  JWT_PRIVATE_KEY: privateKey.export({ type: 'pkcs8', format: 'pem' }).trimEnd(),
  JWKS: JSON.stringify({ keys: [{ ...publicKey.export({ format: 'jwk' }), use: 'sig' }] }),
})) {
  const result = spawnSync(process.execPath, ['node_modules/convex/bin/main.js', 'env', 'set', name, '--', value], { encoding: 'utf8' });
  assert.equal(result.status, 0, `Could not set ${name}: ${result.stderr}`);
}
writeFileSync('convex/auth.config.js', 'export default { providers: [{ domain: process.env.CONVEX_SITE_URL, applicationID: "convex" }] };\n');
console.log('Configured local Convex Auth signing and verification. No external identity service.');
