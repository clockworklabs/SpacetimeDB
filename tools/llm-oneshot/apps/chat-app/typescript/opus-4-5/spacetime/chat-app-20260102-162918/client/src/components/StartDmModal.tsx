import { useState, FormEvent } from 'react';
import { DbConnection, User } from '../module_bindings';
import { Identity } from 'spacetimedb/react';

interface StartDmModalProps {
  conn: DbConnection;
  users: User[];
  myIdentity: Identity | null;
  onClose: () => void;
}

export default function StartDmModal({
  conn,
  users,
  myIdentity,
  onClose,
}: StartDmModalProps) {
  const [targetName, setTargetName] = useState('');
  const [error, setError] = useState('');

  const otherUsers = users.filter(
    u =>
      myIdentity &&
      u.identity.toHexString() !== myIdentity.toHexString() &&
      u.name
  );

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!targetName.trim()) {
      setError('Please select a user');
      return;
    }

    try {
      conn.reducers.startDm({ targetName: targetName.trim() });
      onClose();
    } catch (err) {
      setError('Failed to start DM');
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Start Direct Message</h3>
        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label>Select User</label>
            <select
              className="input"
              value={targetName}
              onChange={e => setTargetName(e.target.value)}
            >
              <option value="">Choose a user...</option>
              {otherUsers.map(user => (
                <option
                  key={user.identity.toHexString()}
                  value={user.name ?? ''}
                >
                  {user.name}
                </option>
              ))}
            </select>
          </div>
          {error && (
            <p style={{ color: 'var(--danger)', fontSize: '14px' }}>{error}</p>
          )}
          <div className="modal-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onClose}
            >
              Cancel
            </button>
            <button type="submit" className="btn btn-primary">
              Start DM
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
