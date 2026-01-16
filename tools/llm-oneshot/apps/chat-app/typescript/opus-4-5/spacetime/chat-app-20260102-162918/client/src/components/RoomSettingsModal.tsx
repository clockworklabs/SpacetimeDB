import { useState } from 'react';
import { DbConnection, Room, RoomMember, User } from '../module_bindings';

interface RoomSettingsModalProps {
  conn: DbConnection;
  room: Room;
  members: RoomMember[];
  users: User[];
  onClose: () => void;
}

export default function RoomSettingsModal({
  conn,
  room,
  members,
  users,
  onClose,
}: RoomSettingsModalProps) {
  const [inviteeName, setInviteeName] = useState('');
  const [error, setError] = useState('');
  const [success, setSuccess] = useState('');

  const handleInvite = () => {
    if (!inviteeName.trim()) {
      setError('Please enter a username');
      return;
    }

    try {
      conn.reducers.inviteToRoom({ roomId: room.id, inviteeName: inviteeName.trim() });
      setSuccess(`Invitation sent to ${inviteeName}`);
      setInviteeName('');
      setError('');
    } catch (err) {
      setError('Failed to send invitation');
    }
  };

  const admins = members.filter(m => m.isAdmin);
  const adminNames = admins.map(
    a => users.find(u => u.identity.toHexString() === a.userId.toHexString())?.name ?? 'Unknown'
  );

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Room Settings</h3>

        <div className="form-group">
          <label>Room Name</label>
          <div style={{ color: 'var(--text-primary)', fontSize: '16px' }}>{room.name}</div>
        </div>

        <div className="form-group">
          <label>Type</label>
          <div style={{ color: 'var(--text-primary)' }}>
            {room.isDm ? 'Direct Message' : room.isPrivate ? 'Private Room' : 'Public Room'}
          </div>
        </div>

        <div className="form-group">
          <label>Admins</label>
          <div style={{ color: 'var(--text-primary)' }}>{adminNames.join(', ')}</div>
        </div>

        <div className="form-group">
          <label>Members</label>
          <div style={{ color: 'var(--text-primary)' }}>{members.length} members</div>
        </div>

        {room.isPrivate && !room.isDm && (
          <div className="form-group">
            <label>Invite User</label>
            <div style={{ display: 'flex', gap: '8px' }}>
              <input
                type="text"
                className="input"
                value={inviteeName}
                onChange={e => setInviteeName(e.target.value)}
                placeholder="Enter username..."
                onKeyDown={e => e.key === 'Enter' && handleInvite()}
              />
              <button className="btn btn-primary" onClick={handleInvite}>
                Invite
              </button>
            </div>
            {error && <p style={{ color: 'var(--danger)', fontSize: '12px', marginTop: '4px' }}>{error}</p>}
            {success && <p style={{ color: 'var(--success)', fontSize: '12px', marginTop: '4px' }}>{success}</p>}
          </div>
        )}

        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
