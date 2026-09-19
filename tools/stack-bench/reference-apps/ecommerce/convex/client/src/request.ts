/// <reference types="vite/client" />
import { ConvexClient, ConvexHttpClient } from 'convex/browser';
import { makeFunctionReference } from 'convex/server';

export const TOKEN_KEY = 'convex_shop_token';
Object.assign(window, { getSessionToken: () => localStorage.getItem(TOKEN_KEY) });
const REFRESH_KEY = 'convex_shop_refresh';
const url = import.meta.env.VITE_CONVEX_URL;
const http = new ConvexHttpClient(url);
const live = new ConvexClient(url);
let refreshing: Promise<string | null> | null = null;
function storeTokens(tokens: { token: string; refreshToken: string }) {
  localStorage.setItem(TOKEN_KEY, tokens.token);
  localStorage.setItem(REFRESH_KEY, tokens.refreshToken);
  http.setAuth(tokens.token);
}
async function accessToken(force = false): Promise<string | null> {
  const token = localStorage.getItem(TOKEN_KEY);
  if (!token) return null;
  const expires = JSON.parse(atob(token.split('.')[1].replace(/-/g, '+').replace(/_/g, '/'))).exp * 1000;
  if (!force && expires > Date.now() + 30000) return token;
  if (!refreshing) refreshing = (async () => {
    const result = await http.action(makeFunctionReference<'action'>('auth:signIn'), { refreshToken: localStorage.getItem(REFRESH_KEY) });
    if (!result.tokens) throw new Error('Session expired');
    storeTokens(result.tokens);
    return result.tokens.token;
  })().finally(() => { refreshing = null; });
  return refreshing;
}
function followAuth() {
  if (localStorage.getItem(TOKEN_KEY)) live.setAuth(({ forceRefreshToken }) => accessToken(forceRefreshToken));
  else live.setAuth(async () => null);
}
followAuth();
export async function signOut() {
  const token = await accessToken();
  if (token) { http.setAuth(token); await http.action(makeFunctionReference<'action'>('auth:signOut'), {}); }
  localStorage.removeItem(TOKEN_KEY); localStorage.removeItem(REFRESH_KEY);
  http.clearAuth(); live.setAuth(async () => null);
}
export function subscribeState(onValue: (state: any) => void) {
  return live.onUpdate(makeFunctionReference<'query'>('shop:state'), {}, onValue);
}
export function subscribeProgression(onValue: (state: any) => void) {
  return live.onUpdate(makeFunctionReference<'query'>('progression:state'), {}, onValue);
}
async function state() { return http.query(makeFunctionReference<'query'>('shop:state'), {}); }

export async function validateSession() {
  const token = await accessToken();
  if (!token) throw new Error('Sign in required');
  http.setAuth(token);
  if (!(await state()).user) throw new Error('Sign in required');
}
export async function authenticate(username: string, password: string, flow: 'signUp' | 'signIn') {
  const result = await http.action(makeFunctionReference<'action'>('auth:signIn'), {
    provider: 'password', params: { username, password, flow },
  });
  if (!result.tokens) throw new Error('Sign in failed');
  storeTokens(result.tokens); followAuth();
  return { token: result.tokens.token, user: (await state()).user };
}
export async function mutate(name: string, args: Record<string, unknown> = {}) {
  const token = await accessToken();
  if (token) http.setAuth(token); else http.clearAuth();
  return http.mutation(makeFunctionReference<'mutation'>(name), args);
}
