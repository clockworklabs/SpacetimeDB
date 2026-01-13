import { useState, FormEvent } from 'react';
import { DbConnection } from '../module_bindings';

interface UserSetupProps {
  conn: DbConnection;
}

export default function UserSetup({ conn }: UserSetupProps) {
  const [name, setName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('Please enter a name');
      return;
    }

    setIsSubmitting(true);
    setError('');

    try {
      conn.reducers.setName({ name: name.trim() });
    } catch (err) {
      setError('Failed to set name');
      setIsSubmitting(false);
    }
  };

  return (
    <div className="user-setup">
      <h1>Welcome to Chat App</h1>
      <p>Choose a display name to get started</p>
      <form onSubmit={handleSubmit}>
        <input
          type="text"
          className="input"
          placeholder="Enter your name..."
          value={name}
          onChange={(e) => setName(e.target.value)}
          maxLength={50}
          autoFocus
        />
        {error && <p style={{ color: 'var(--danger)', fontSize: '14px' }}>{error}</p>}
        <button type="submit" className="btn btn-primary" disabled={isSubmitting}>
          {isSubmitting ? 'Setting name...' : 'Continue'}
        </button>
      </form>
    </div>
  );
}
