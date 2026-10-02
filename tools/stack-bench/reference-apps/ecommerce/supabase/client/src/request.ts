/// <reference types="vite/client" />
import { createClient, type RealtimeChannel, type Session } from '@supabase/supabase-js';

const STORAGE_KEY = 'storefront-auth';
const supabase = createClient(import.meta.env.VITE_SUPABASE_URL, import.meta.env.VITE_SUPABASE_ANON_KEY,
  { auth: { storageKey: STORAGE_KEY } });

const hex = (bytes: Uint8Array) => [...bytes].map(byte => byte.toString(16).padStart(2, '0')).join('');
// Auth identifies accounts by email, which ignores case; the hex form keeps usernames distinct.
const accountEmail = (username: string) => `${hex(new TextEncoder().encode(username))}@accounts.invalid`;
// Auth accepts 6 to 72 bytes. Its digest lets every password up to 64 characters count in full.
async function accountPassword(password: string) {
  return hex(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(password))));
}

function storedSession(): Session | null {
  try { return JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null'); } catch { return null; }
}
let session = storedSession();
export const sessionToken = () => session?.access_token ?? null;
Object.assign(window, { getSessionToken: sessionToken });

export async function mutate(name: string, args: Record<string, unknown> = {}) {
  const { data, error } = await supabase.rpc(name, args);
  if (error) throw new Error(error.message);
  return data;
}

// Every state reader reloads when Realtime reports a change it may see. Changes flow only
// after the postgres_changes "ok" system message, which also follows each reconnect.
const readers = new Set<() => void>();
const reloadAll = () => readers.forEach(reload => reload());
let changes: RealtimeChannel | null = null;
let joins = 0;
// Realtime checks row access with the token the channel joined with, so rejoin as each account.
function followChanges() {
  if (changes) supabase.removeChannel(changes);
  changes = supabase.channel(`storefront-${++joins}`)
    .on('postgres_changes', { event: '*', schema: 'public' }, reloadAll)
    .on('system', {}, (message: { extension?: string; status?: string }) => {
      if (message.extension === 'postgres_changes' && message.status === 'ok') reloadAll();
    })
    .subscribe();
}
supabase.auth.onAuthStateChange((event, next) => {
  session = next;
  // Auth callbacks must not call Supabase directly.
  if (['INITIAL_SESSION', 'SIGNED_IN', 'SIGNED_OUT'].includes(event)) setTimeout(followChanges);
});

function live(name: string, onValue: (state: any) => void) {
  let closed = false, running = false, stale = false;
  const reload = async (): Promise<void> => {
    if (running) { stale = true; return; }
    running = true;
    stale = false;
    let failed = false;
    try {
      const state = await mutate(name);
      if (!closed) onValue(state);
    } catch { failed = true; }
    running = false;
    if (closed) return;
    if (failed) setTimeout(reload, 1000);
    else if (stale) reload();
  };
  readers.add(reload);
  reload();
  return () => { closed = true; readers.delete(reload); };
}
export const subscribeState = (onValue: (state: any) => void) => live('shop_state', onValue);
export const subscribeProgression = (onValue: (state: any) => void) => live('progression_state', onValue);

export async function validateSession() {
  const { data } = await supabase.auth.getSession();
  if (data.session && (await mutate('shop_state')).user) return;
  await supabase.auth.signOut({ scope: 'local' });
  throw new Error('Sign in required');
}

export async function authenticate(username: string, password: string, flow: 'signUp' | 'signIn') {
  if (flow === 'signUp' && !/^[A-Za-z0-9-]{1,48}$/.test(username)) {
    throw new Error('Use up to 48 letters, digits, and hyphens');
  }
  if ([...password].length > 64) throw new Error('Use a password of up to 64 characters');
  const credentials = { email: accountEmail(username), password: await accountPassword(password) };
  const { data, error } = flow === 'signUp'
    ? await supabase.auth.signUp(credentials)
    : await supabase.auth.signInWithPassword(credentials);
  if (error || !data.session) {
    if (flow === 'signIn') throw new Error('Invalid username or password');
    throw new Error(error?.code === 'user_already_exists' ? 'Username already exists' : 'Sign up failed');
  }
  session = data.session;
  return { token: data.session.access_token, user: (await mutate('shop_state')).user };
}

export async function signOut() {
  await supabase.auth.signOut({ scope: 'local' });
  session = null;
}
