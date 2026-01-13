import { useState } from 'react';
import { DbConnection, tables, Room, User } from '../module_bindings';
import { Identity, useTable } from 'spacetimedb/react';
import CreateRoomModal from './CreateRoomModal';
import StartDmModal from './StartDmModal';
import StatusDropdown from './StatusDropdown';

interface SidebarProps {
  conn: DbConnection;
  rooms: Room[];
  myRoomIds: Set<bigint>;
  selectedRoomId: bigint | null;
  onSelectRoom: (roomId: bigint | null) => void;
  currentUser: User;
  users: User[];
  pendingInvitesCount: number;
  onShowInvites: () => void;
  myIdentity: Identity | null;
}

export default function Sidebar({
  conn,
  rooms,
  myRoomIds,
  selectedRoomId,
  onSelectRoom,
  currentUser,
  users,
  pendingInvitesCount,
  onShowInvites,
  myIdentity,
}: SidebarProps) {
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showStartDm, setShowStartDm] = useState(false);

  const [messages] = useTable(tables.message);
  const [readReceipts] = useTable(tables.readReceipt);

  // Calculate unread counts per room
  const unreadCounts = new Map<bigint, number>();
  if (myIdentity && messages && readReceipts) {
    for (const room of rooms) {
      if (!myRoomIds.has(room.id)) continue;

      const roomMessages = messages.filter(m => m.roomId === room.id);
      const myReadMessageIds = new Set(
        readReceipts
          .filter(r => r.userId.toHexString() === myIdentity.toHexString() && r.roomId === room.id)
          .map(r => r.messageId)
      );

      let unread = 0;
      for (const msg of roomMessages) {
        if (msg.senderId.toHexString() !== myIdentity.toHexString() && !myReadMessageIds.has(msg.id)) {
          unread++;
        }
      }
      if (unread > 0) {
        unreadCounts.set(room.id, unread);
      }
    }
  }

  // Separate DMs from regular rooms
  const dmRooms = rooms.filter(r => r.isDm && myRoomIds.has(r.id));
  const publicRooms = rooms.filter(r => !r.isDm && !r.isPrivate);
  const privateRooms = rooms.filter(r => !r.isDm && r.isPrivate && myRoomIds.has(r.id));

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <h2>Chat App</h2>
        {pendingInvitesCount > 0 && (
          <button className="btn btn-small btn-primary" onClick={onShowInvites}>
            {pendingInvitesCount} Invite{pendingInvitesCount > 1 ? 's' : ''}
          </button>
        )}
      </div>

      <div className="room-list">
        {/* Direct Messages */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Direct Messages</span>
            <button className="btn-icon" onClick={() => setShowStartDm(true)} title="New DM">
              +
            </button>
          </div>
          {dmRooms.length === 0 ? (
            <p style={{ fontSize: '12px', color: 'var(--text-muted)', padding: '8px 12px' }}>
              No DMs yet
            </p>
          ) : (
            dmRooms.map(room => (
              <RoomItem
                key={room.id.toString()}
                room={room}
                isSelected={selectedRoomId === room.id}
                isMember={true}
                unreadCount={unreadCounts.get(room.id) ?? 0}
                onClick={() => onSelectRoom(room.id)}
              />
            ))
          )}
        </div>

        {/* Private Rooms */}
        {privateRooms.length > 0 && (
          <div className="sidebar-section">
            <div className="sidebar-section-header">
              <span>Private Rooms</span>
            </div>
            {privateRooms.map(room => (
              <RoomItem
                key={room.id.toString()}
                room={room}
                isSelected={selectedRoomId === room.id}
                isMember={true}
                unreadCount={unreadCounts.get(room.id) ?? 0}
                onClick={() => onSelectRoom(room.id)}
              />
            ))}
          </div>
        )}

        {/* Public Rooms */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="btn-icon" onClick={() => setShowCreateRoom(true)} title="Create Room">
              +
            </button>
          </div>
          {publicRooms.length === 0 ? (
            <p style={{ fontSize: '12px', color: 'var(--text-muted)', padding: '8px 12px' }}>
              No rooms yet. Create one!
            </p>
          ) : (
            publicRooms.map(room => (
              <RoomItem
                key={room.id.toString()}
                room={room}
                isSelected={selectedRoomId === room.id}
                isMember={myRoomIds.has(room.id)}
                unreadCount={unreadCounts.get(room.id) ?? 0}
                onClick={() => onSelectRoom(room.id)}
              />
            ))
          )}
        </div>

        {/* Online Users */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Online — {users.filter(u => u.online && u.status !== 'invisible').length}</span>
          </div>
          {users
            .filter(u => u.online && u.status !== 'invisible')
            .map(user => (
              <div key={user.identity.toHexString()} className="member-item">
                <div className="member-avatar">
                  {(user.name ?? '?')[0].toUpperCase()}
                  <div className={`status-dot ${user.status}`}></div>
                </div>
                <div className="member-info">
                  <div className="member-name">{user.name ?? 'Anonymous'}</div>
                </div>
              </div>
            ))}
        </div>
      </div>

      <div className="user-panel">
        <StatusDropdown conn={conn} currentUser={currentUser} />
      </div>

      {showCreateRoom && (
        <CreateRoomModal conn={conn} onClose={() => setShowCreateRoom(false)} />
      )}

      {showStartDm && (
        <StartDmModal conn={conn} users={users} myIdentity={myIdentity} onClose={() => setShowStartDm(false)} />
      )}
    </div>
  );
}

interface RoomItemProps {
  room: Room;
  isSelected: boolean;
  isMember: boolean;
  unreadCount: number;
  onClick: () => void;
}

function RoomItem({ room, isSelected, isMember, unreadCount, onClick }: RoomItemProps) {
  return (
    <div
      className={`room-item ${isSelected ? 'selected' : ''}`}
      onClick={onClick}
    >
      <span className="room-icon">
        {room.isDm ? '💬' : room.isPrivate ? '🔒' : '#'}
      </span>
      <span className="room-name">{room.name}</span>
      {!isMember && <span style={{ fontSize: '11px', color: 'var(--text-muted)' }}>Join</span>}
      {unreadCount > 0 && <span className="unread-badge">{unreadCount > 99 ? '99+' : unreadCount}</span>}
    </div>
  );
}
