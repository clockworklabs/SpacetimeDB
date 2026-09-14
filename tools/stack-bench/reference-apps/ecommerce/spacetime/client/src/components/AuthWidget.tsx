import { useState } from 'react';

interface AuthWidgetProps {
  currentUsername: string | null;
  onSignUp: (username: string, password: string) => Promise<void>;
  onSignIn: (username: string, password: string) => Promise<void>;
  onSignOut: () => void;
}

export default function AuthWidget({
  currentUsername,
  onSignUp,
  onSignIn,
  onSignOut,
}: AuthWidgetProps) {
  const [showSignIn, setShowSignIn] = useState(false);
  const [signUpUsername, setSignUpUsername] = useState('');
  const [signUpPassword, setSignUpPassword] = useState('');
  const [signInUsername, setSignInUsername] = useState('');
  const [signInPassword, setSignInPassword] = useState('');
  const [error, setError] = useState<string | null>(null);

  if (currentUsername) {
    return (
      <div className="auth-widget">
        <span className="current-user" data-role="current-user">
          {currentUsername}
        </span>
        <span className="current-user" data-role="staff-current-user">{currentUsername}</span>
        <button type="button" className="btn btn-ghost btn-sm" data-role="signout" onClick={onSignOut}>
          Sign out
        </button>
      </div>
    );
  }

  const submitSignUp = async () => {
    setError(null);
    try {
      await onSignUp(signUpUsername.trim(), signUpPassword);
      setSignUpUsername('');
      setSignUpPassword('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign up failed.');
    }
  };

  const submitSignIn = async () => {
    setError(null);
    try {
      await onSignIn(signInUsername.trim(), signInPassword);
      setSignInUsername('');
      setSignInPassword('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign in failed.');
    }
  };

  return (
    <div className="auth-forms">
      <div className="auth-form">
        <input
          type="text"
          data-role="signup-username"
          placeholder="Username"
          value={signUpUsername}
          onChange={(e) => setSignUpUsername(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submitSignUp();
          }}
        />
        <input
          type="password"
          data-role="signup-password"
          placeholder="Password"
          value={signUpPassword}
          onChange={(e) => setSignUpPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submitSignUp();
          }}
        />
        <button type="button" className="btn btn-primary btn-sm" data-role="signup-submit" onClick={submitSignUp}>
          Sign up
        </button>
        {!showSignIn && (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            data-role="signin-toggle"
            onClick={() => setShowSignIn(true)}
          >
            Sign in instead
          </button>
        )}
      </div>

      {showSignIn && (
        <div className="auth-form">
          <input
            type="text"
            data-role="signin-username"
            placeholder="Username"
            value={signInUsername}
            onChange={(e) => setSignInUsername(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submitSignIn();
            }}
          />
          <input
            type="password"
            data-role="signin-password"
            placeholder="Password"
            value={signInPassword}
            onChange={(e) => setSignInPassword(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') submitSignIn();
            }}
          />
          <button type="button" className="btn btn-primary btn-sm" data-role="signin-submit" onClick={submitSignIn}>
            Sign in
          </button>
        </div>
      )}

      <div className="auth-form staff-auth-form">
        <input type="text" data-role="staff-signin-username" placeholder="Staff username" value={signInUsername} onChange={(e) => setSignInUsername(e.target.value)} />
        <input type="password" data-role="staff-signin-password" placeholder="Staff password" value={signInPassword} onChange={(e) => setSignInPassword(e.target.value)} />
        <button type="button" className="btn btn-ghost btn-sm" data-role="staff-signin-submit" onClick={submitSignIn}>Staff sign in</button>
      </div>

      {error && (
        <div className="error-text" data-role="auth-error">
          {error}
        </div>
      )}
    </div>
  );
}
