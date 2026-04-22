import { useState, FormEvent } from 'react';
import { DbConnection } from '../module_bindings';

interface CreateRoomModalProps {
  conn: DbConnection;
  onClose: () => void;
}

export default function CreateRoomModal({
  conn,
  onClose,
}: CreateRoomModalProps) {
  const [name, setName] = useState('');
  const [isPrivate, setIsPrivate] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('Room name is required');
      return;
    }

    try {
      conn.reducers.createRoom({ name: name.trim(), isPrivate });
      onClose();
    } catch (err) {
      setError('Failed to create room');
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Create Room</h3>
        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label>Room Name</label>
            <input
              type="text"
              className="input"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="Enter room name..."
              maxLength={100}
              autoFocus
            />
          </div>
          <div className="form-group">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={isPrivate}
                onChange={e => setIsPrivate(e.target.checked)}
              />
              Private room (invite only)
            </label>
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
              Create Room
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
