import React, { useState } from 'react';

interface UserSetupProps {
  onUserCreated: () => void;
}

export default function UserSetup({ onUserCreated }: UserSetupProps) {
  const [displayName, setDisplayName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!displayName.trim()) return;

    setIsSubmitting(true);
    try {
      if (window.__db_conn) {
        await window.__db_conn.reducers.setDisplayName({
          displayName: displayName.trim(),
        });
        onUserCreated();
      }
    } catch (error) {
      console.error('Failed to set display name:', error);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        backgroundColor: 'var(--bg-primary)',
        color: 'var(--text-primary)',
      }}
    >
      <div
        style={{
          backgroundColor: 'var(--bg-secondary)',
          padding: '32px',
          borderRadius: '8px',
          boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
          maxWidth: '400px',
          width: '100%',
        }}
      >
        <h2 style={{ marginBottom: '24px', textAlign: 'center' }}>
          Welcome to SpacetimeDB Chat
        </h2>
        <form onSubmit={handleSubmit}>
          <div style={{ marginBottom: '16px' }}>
            <label
              style={{
                display: 'block',
                marginBottom: '8px',
                fontWeight: '500',
              }}
            >
              Choose a display name:
            </label>
            <input
              type="text"
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              className="input"
              placeholder="Enter your display name"
              maxLength={50}
              style={{ width: '100%' }}
              disabled={isSubmitting}
            />
          </div>
          <button
            type="submit"
            className="btn btn-primary"
            style={{ width: '100%' }}
            disabled={!displayName.trim() || isSubmitting}
          >
            {isSubmitting ? 'Setting up...' : 'Join Chat'}
          </button>
        </form>
      </div>
    </div>
  );
}
