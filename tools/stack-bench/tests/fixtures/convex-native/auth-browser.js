import { ConvexClient } from 'convex/browser';
const client = new ConvexClient(window.DEPLOYMENT_URL, { unsavedChangesWarning: false });
let token = sessionStorage.getItem('session');
window.getSessionToken = () => token;
if (token) client.setAuth(async () => token);
const status = document.querySelector('#status');
const current = document.querySelector('#current-user');
async function showUser() {
  const user = await client.query('accountShop:current', {});
  current.textContent = user.name;
  current.hidden = false;
}
if (token) showUser().catch(() => { status.textContent = 'Session rejected'; });
document.querySelector('form').onsubmit = async (event) => {
  event.preventDefault();
  try {
    const result = await client.action('auth:signIn', { provider: 'password', params: {
      username: document.querySelector('#signup-username').value,
      password: document.querySelector('#signup-password').value,
      flow: event.submitter.value,
    }});
    token = result.tokens.token;
    sessionStorage.setItem('session', token);
    client.setAuth(async () => token);
    await showUser();
    status.textContent = 'Signed in';
  } catch { status.textContent = 'Sign in rejected'; }
};
document.querySelector('#purchase').onclick = async () => {
  try {
    await client.mutation('accountShop:purchase', { itemId: window.ITEM_ID, quantity: 1 });
    status.textContent = 'Purchased';
  } catch { status.textContent = 'Purchase rejected'; }
};
document.querySelector('#signout').onclick = async () => {
  await client.action('auth:signOut', {});
  token = null;
  sessionStorage.removeItem('session');
  client.setAuth(async () => null);
  current.hidden = true;
  status.textContent = 'Signed out';
};
