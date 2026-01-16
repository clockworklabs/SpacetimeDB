import { DbConnection, RoomInvite, Room, User } from '../module_bindings';

interface InvitesPanelProps {
  conn: DbConnection;
  invites: RoomInvite[];
  rooms: Room[];
  users: User[];
  onClose: () => void;
}

export default function InvitesPanel({ conn, invites, rooms, users, onClose }: InvitesPanelProps) {
  const handleAccept = (inviteId: bigint) => {
    conn.reducers.respondToInvite({ inviteId, accept: true });
  };

  const handleDecline = (inviteId: bigint) => {
    conn.reducers.respondToInvite({ inviteId, accept: false });
  };

  return (
    <div className="invites-panel">
      <div className="invites-header">
        <h2>Pending Invitations</h2>
        <button className="btn btn-secondary" onClick={onClose}>
          Close
        </button>
      </div>
      <div className="invites-list">
        {invites.length === 0 ? (
          <p style={{ textAlign: 'center', color: 'var(--text-muted)', padding: '32px' }}>
            No pending invitations
          </p>
        ) : (
          invites.map(invite => {
            const room = rooms.find(r => r.id === invite.roomId);
            const inviter = users.find(u => u.identity.toHexString() === invite.inviterId.toHexString());
            return (
              <div key={invite.id.toString()} className="invite-item">
                <div className="invite-info">
                  <h4>{room?.name ?? 'Unknown Room'}</h4>
                  <p>Invited by {inviter?.name ?? 'Unknown'}</p>
                </div>
                <div className="invite-actions">
                  <button className="btn btn-primary btn-small" onClick={() => handleAccept(invite.id)}>
                    Accept
                  </button>
                  <button className="btn btn-secondary btn-small" onClick={() => handleDecline(invite.id)}>
                    Decline
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
