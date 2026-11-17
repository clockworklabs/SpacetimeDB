import assert from 'node:assert/strict';
import { Request } from 'spacetimedb/server';
import { client } from '../src/client';
import type { AuthHandlerCtx } from '../src/context';

const routes = client({
  sendMail: () => {
    throw new Error('unexpected mail');
  },
});
const ctx = new Proxy({} as AuthHandlerCtx, {
  get() {
    throw new Error('invalid input reached the database');
  },
});
const credentials = { email: 'user@example.com', password: 'password123' };
const cases = [
  { handler: routes.passwordSignup, body: { ...credentials, email: 42 } },
  { handler: routes.passwordSignup, body: { ...credentials, password: [] } },
  { handler: routes.passwordSignup, body: { ...credentials, name: null } },
  { handler: routes.passwordLogin, body: { ...credentials, email: {} } },
  { handler: routes.passwordLogin, body: { ...credentials, password: true } },
  { handler: routes.forgotPassword, body: { email: [] } },
  {
    handler: routes.resetPassword,
    body: { token: {}, newPassword: 'password123' },
  },
  { handler: routes.resetPassword, body: { token: 'token', newPassword: 42 } },
];
for (const { handler, body } of cases) {
  const request = new Request('http://localhost/auth', {
    method: 'POST',
    body: JSON.stringify(body),
  });
  assert.equal(handler(ctx, request).status, 400);
}
process.stdout.write('Auth HTTP input validation passed\n');
