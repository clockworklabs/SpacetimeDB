import { useState } from 'react';
import { DbConnection, RoomMember, User, RoomBan } from '../module_bindings';
import { Identity } from 'spacetimedb/react';

interface MembersPanelProps {
  conn: DbConnection;
  roomId: bigint;
  members: RoomMember[];
  users: User[];
  bans: RoomBan[];
  isAdmin: boolean;
  myIdentity: Identity | null;
}

export default function MembersPanel({
  conn,
  roomId,
  members,
  users,
  bans,
  isAdmin,
  myIdentity,
}: MembersPanelProps) {
  const [showInvite, setShowInvite] = useState(false);
  const [inviteeName, setInviteeName] = useState('');

  const memberDetails = members.map(m => {
    const user = users.find(
      u => u.identity.toHexString() === m.userId.toHexString()
    );
    return { member: m, user };
  });

  const onlineMembers = memberDetails.filter(
    ({ user }) => user?.online && user.status !== 'invisible'
  );
  const offlineMembers = memberDetails.filter(
    ({ user }) => !user?.online || user.status === 'invisible'
  );

  const handleKick = (userId: Identity) => {
    if (confirm('Kick this user from the room?')) {
      conn.reducers.kickUser({ roomId, userId });
    }
  };

  const handleBan = (userId: Identity) => {
    if (confirm('Ban this user from the room?')) {
      conn.reducers.banUser({ roomId, userId });
    }
  };

  const handlePromote = (userId: Identity) => {
    if (confirm('Promote this user to admin?')) {
      conn.reducers.promoteToAdmin({ roomId, userId });
    }
  };

  const handleInvite = () => {
    if (inviteeName.trim()) {
      conn.reducers.inviteToRoom({ roomId, inviteeName: inviteeName.trim() });
      setInviteeName('');
      setShowInvite(false);
    }
  };

  const formatLastActive = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    const now = Date.now();
    const diff = now - date.getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  };

  return (
    <div className="members-panel">
      {isAdmin && (
        <div style={{ marginBottom: '16px' }}>
          {showInvite ? (
            <div
              style={{ display: 'flex', gap: '8px', flexDirection: 'column' }}
            >
              <input
                type="text"
                className="input"
                placeholder="Username to invite..."
                value={inviteeName}
                onChange={e => setInviteeName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleInvite()}
              />
              <div style={{ display: 'flex', gap: '4px' }}>
                <button
                  className="btn btn-primary btn-small"
                  onClick={handleInvite}
                >
                  Invite
                </button>
                <button
                  className="btn btn-secondary btn-small"
                  onClick={() => setShowInvite(false)}
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button
              className="btn btn-secondary btn-small"
              onClick={() => setShowInvite(true)}
              style={{ width: '100%' }}
            >
              + Invite User
            </button>
          )}
        </div>
      )}

      <div className="members-section">
        <h4>Online — {onlineMembers.length}</h4>
        {onlineMembers.map(({ member, user }) => (
          <MemberItem
            key={member.id.toString()}
            member={member}
            user={user}
            isAdmin={isAdmin}
            isMe={myIdentity?.toHexString() === member.userId.toHexString()}
            onKick={() => handleKick(member.userId)}
            onBan={() => handleBan(member.userId)}
            onPromote={() => handlePromote(member.userId)}
          />
        ))}
      </div>

      <div className="members-section">
        <h4>Offline — {offlineMembers.length}</h4>
        {offlineMembers.map(({ member, user }) => (
          <MemberItem
            key={member.id.toString()}
            member={member}
            user={user}
            isAdmin={isAdmin}
            isMe={myIdentity?.toHexString() === member.userId.toHexString()}
            onKick={() => handleKick(member.userId)}
            onBan={() => handleBan(member.userId)}
            onPromote={() => handlePromote(member.userId)}
            lastActive={user ? formatLastActive(user.lastActive) : undefined}
          />
        ))}
      </div>

      {bans.length > 0 && isAdmin && (
        <div className="members-section">
          <h4>Banned — {bans.length}</h4>
          {bans.map(ban => {
            const user = users.find(
              u => u.identity.toHexString() === ban.userId.toHexString()
            );
            return (
              <div key={ban.id.toString()} className="member-item">
                <div className="member-info">
                  <div
                    className="member-name"
                    style={{ color: 'var(--danger)' }}
                  >
                    {user?.name ?? 'Unknown'}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

interface MemberItemProps {
  member: RoomMember;
  user: User | undefined;
  isAdmin: boolean;
  isMe: boolean;
  onKick: () => void;
  onBan: () => void;
  onPromote: () => void;
  lastActive?: string;
}

function MemberItem({
  member,
  user,
  isAdmin,
  isMe,
  onKick,
  onBan,
  onPromote,
  lastActive,
}: MemberItemProps) {
  const [showActions, setShowActions] = useState(false);

  return (
    <div
      className="member-item"
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <div className="member-avatar">
        {(user?.name ?? '?')[0].toUpperCase()}
        <div className={`status-dot ${user?.status ?? 'offline'}`}></div>
      </div>
      <div className="member-info">
        <div className="member-name">
          {user?.name ?? 'Unknown'}
          {member.isAdmin && ' 👑'}
        </div>
        {lastActive && <div className="member-role">{lastActive}</div>}
      </div>

      {isAdmin && !isMe && showActions && (
        <div style={{ display: 'flex', gap: '4px' }}>
          {!member.isAdmin && (
            <button
              className="btn-icon btn-small"
              onClick={onPromote}
              title="Promote to admin"
            >
              👑
            </button>
          )}
          <button className="btn-icon btn-small" onClick={onKick} title="Kick">
            👢
          </button>
          <button className="btn-icon btn-small" onClick={onBan} title="Ban">
            🚫
          </button>
        </div>
      )}
    </div>
  );
}
