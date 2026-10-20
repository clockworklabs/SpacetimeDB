// Disposable C0 identities only. This is not an account implementation or qualification.
import { generateKeyPairSync, sign } from 'node:crypto';
import { writeFileSync } from 'node:fs';
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...publicKey.export({ format: 'jwk' }), kid: 'c0', alg: 'RS256', use: 'sig' };
const issuer = 'https://stackbench-c0.invalid';
const audience = 'stackbench-c0';
const jwks = 'data:text/plain;charset=utf-8;base64,' + Buffer.from(JSON.stringify({ keys: [jwk] })).toString('base64');
writeFileSync('convex/auth.config.js', `export default ${JSON.stringify({ providers: [{ type: 'customJwt', issuer, applicationID: audience, algorithm: 'RS256', jwks }] })};\n`);
const b64 = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
const token = (sub) => {
  const iat = Math.floor(Date.now() / 1000);
  const payload = b64({ alg: 'RS256', typ: 'JWT', kid: 'c0' }) + '.' + b64({ iss: issuer, aud: audience, sub, iat, exp: iat + 7200 });
  return payload + '.' + sign('RSA-SHA256', Buffer.from(payload), privateKey).toString('base64url');
};
writeFileSync('identities.json', JSON.stringify({ buyer: token('buyer'), observer: token('observer') }), { mode: 0o600 });
console.log('Wrote public auth configuration and two disposable signed actor tokens.');
