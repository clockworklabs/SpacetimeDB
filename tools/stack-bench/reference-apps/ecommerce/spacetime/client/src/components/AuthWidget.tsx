import { useState } from 'react';
import { useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection } from '../module_bindings';
import { authenticate, clearToken, readAuthError } from '../auth';

export default function AuthWidget({ currentUsername }: { currentUsername: string | null }) {
  const { getConnection } = useSpacetimeDB();
  const [mode, setMode] = useState<'signup' | 'signin' | null>(null);
  const [name, setName] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(readAuthError);
  const perform = async (action: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await action(); }
    catch (error) { setError(error instanceof Error ? error.message : 'Login failed.'); }
    finally { setBusy(false); }
  };
  const logout = async () => {
    const connection = getConnection() as DbConnection | null;
    if (!connection) throw new Error('Not connected.');
    await connection.reducers.signOut({});
    clearToken(); location.reload();
  };
  return <div className="auth-widget">
    {currentUsername ? <>
      <span className="current-user" data-role="current-user">{currentUsername}</span>
      <span className="current-user" data-role="staff-current-user">{currentUsername}</span>
      <button type="button" disabled={busy} className="btn btn-ghost btn-sm" data-role="signout" onClick={() => void perform(logout)}>Sign out</button>
    </> : <>
      <button type="button" data-role="signup-toggle" onClick={() => setMode('signup')}>Create account</button>
      <button type="button" data-role="signin-toggle" onClick={() => setMode('signin')}>Sign in</button>
      {mode && <form onSubmit={event => { event.preventDefault(); void perform(() => authenticate(mode, name, password)); }}>
        <label>Username<input data-role={`${mode}-username`} autoComplete="username" maxLength={48} value={name} onChange={event => setName(event.target.value)} required /></label>
        <label>Password<input data-role={`${mode}-password`} type="password" autoComplete={mode === 'signup' ? 'new-password' : 'current-password'} maxLength={64} value={password} onChange={event => setPassword(event.target.value)} required /></label>
        <button data-role={`${mode}-submit`} disabled={busy} type="submit">{mode === 'signup' ? 'Create account' : 'Sign in'}</button>
      </form>}
    </>}
    {error && <div role="alert" data-role="auth-error">{error}</div>}
  </div>;
}
