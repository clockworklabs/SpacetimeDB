import React from 'react';
import { OnlineUser } from '../types';

interface OnlineUsersProps {
  onlineUsers: OnlineUser[];
}

function OnlineUsers({ onlineUsers }: OnlineUsersProps) {
  return (
    <div
      style={{
        borderTop: '1px solid var(--border)',
        padding: '0.5rem',
      }}
    >
      <h4
        style={{
          color: 'var(--text-secondary)',
          fontSize: '0.9rem',
          marginBottom: '0.5rem',
          padding: '0 0.5rem',
        }}
      >
        Online ({onlineUsers.length})
      </h4>

      <div
        style={{
          maxHeight: '200px',
          overflowY: 'auto',
        }}
      >
        {onlineUsers.map(user => (
          <div
            key={user.userId}
            style={{
              padding: '0.5rem 1rem',
              display: 'flex',
              alignItems: 'center',
              gap: '0.5rem',
            }}
          >
            <div
              style={{
                width: '8px',
                height: '8px',
                borderRadius: '50%',
                background: 'var(--success)',
                flexShrink: 0,
              }}
            />
            <span
              style={{
                color: 'var(--text-primary)',
                fontSize: '0.9rem',
              }}
            >
              {user.displayName}
            </span>
          </div>
        ))}

        {onlineUsers.length === 0 && (
          <div
            style={{
              padding: '1rem',
              color: 'var(--text-muted)',
              textAlign: 'center',
              fontSize: '0.9rem',
            }}
          >
            No users online
          </div>
        )}
      </div>
    </div>
  );
}

export default OnlineUsers;
