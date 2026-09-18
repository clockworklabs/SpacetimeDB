import { useState } from 'react';
import { auth, logout } from '../auth';

export default function AuthWidget({ currentUsername }: { currentUsername: string | null }) {
  const [error, setError] = useState<string | null>(null);
  const perform = (action: () => Promise<void>) => {
    setError(null);
    void action().catch(error => setError(error instanceof Error ? error.message : 'Login failed.'));
  };
  return <div className="auth-widget">
    {currentUsername ? <>
      <span className="current-user" data-role="current-user">{currentUsername}</span>
      <span className="current-user" data-role="staff-current-user">{currentUsername}</span>
      <button type="button" className="btn btn-ghost btn-sm" data-role="signout" onClick={() => perform(logout)}>Sign out</button>
    </> : <>
      <button type="button" className="btn btn-ghost btn-sm" data-role="signup-toggle" data-auth-provider="keycloak"
        onClick={() => perform(() => auth.signinRedirect())}>Create account</button>
      <button type="button" className="btn btn-ghost btn-sm" data-role="signin-toggle" data-auth-provider="keycloak"
        onClick={() => perform(() => auth.signinRedirect())}>Sign in</button>
    </>}
    {error && <div role="alert" data-role="auth-error">{error}</div>}
  </div>;
}
