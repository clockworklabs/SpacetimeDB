import { useState } from 'react';
import { useTable } from 'spacetimedb/react';
import { tables } from '../module_bindings';

interface SidebarProps {
  currentRoomId: bigint | null;
  onRoomSelect: (roomId: bigint) => void;
  currentUser: any;
  users: readonly any[];
  userStatuses: readonly any[];
}

export default function Sidebar({
  currentRoomId,
  onRoomSelect,
  currentUser,
  users,
  userStatuses,
}: SidebarProps) {
  const [newRoomName, setNewRoomName] = useState('');
  const [isCreatingRoom, setIsCreatingRoom] = useState(false);

  // Get rooms and memberships
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [_readReceipts] = useTable(tables.readReceipt);

  // Get rooms the current user is a member of
  const myRoomMemberships = roomMembers.filter(
    member =>
      member.userId.toHexString() === currentUser?.identity.toHexString()
  );

  const myRoomIds = new Set(myRoomMemberships.map(m => m.roomId));
  const myRooms = rooms.filter(room => myRoomIds.has(room.id));

  // Calculate unread counts
  const getUnreadCount = (roomId: bigint) => {
    const membership = myRoomMemberships.find(m => m.roomId === roomId);
    if (!membership) return 0;

    const lastReadId = membership.lastReadMessageId;
    const roomMessages = messages.filter(m => m.roomId === roomId);

    if (!lastReadId) return roomMessages.length;

    const unreadMessages = roomMessages.filter(m => m.id > lastReadId);
    return unreadMessages.length;
  };

  const handleCreateRoom = async () => {
    if (!newRoomName.trim()) return;

    setIsCreatingRoom(true);
    try {
      if (window.__db_conn) {
        await window.__db_conn.reducers.createRoom({
          name: newRoomName.trim(),
          description: '',
          isPublic: true,
        });
        setNewRoomName('');
      }
    } catch (error) {
      console.error('Failed to create room:', error);
    } finally {
      setIsCreatingRoom(false);
    }
  };

  const handleJoinRoom = async (roomId: bigint) => {
    try {
      if (window.__db_conn) {
        await window.__db_conn.reducers.joinRoom({ roomId });
      }
    } catch (error) {
      console.error('Failed to join room:', error);
    }
  };

  // Get online users
  const onlineUsers = users.filter(user => {
    const status = userStatuses.find(
      s => s.identity.toHexString() === user.identity.toHexString()
    );
    return status?.isOnline;
  });

  return (
    <div className="sidebar">
      {/* Room Creation */}
      <div
        style={{
          padding: '16px',
          borderBottom: '1px solid var(--border-color)',
        }}
      >
        <div style={{ marginBottom: '8px' }}>
          <input
            type="text"
            value={newRoomName}
            onChange={e => setNewRoomName(e.target.value)}
            placeholder="Create new room..."
            className="input"
            style={{ width: '100%', marginBottom: '8px' }}
            disabled={isCreatingRoom}
          />
          <button
            onClick={handleCreateRoom}
            className="btn btn-primary"
            style={{ width: '100%' }}
            disabled={!newRoomName.trim() || isCreatingRoom}
          >
            {isCreatingRoom ? 'Creating...' : 'Create Room'}
          </button>
        </div>
      </div>

      {/* My Rooms */}
      <div className="room-list">
        <div
          style={{
            padding: '8px 16px',
            fontSize: '12px',
            color: 'var(--text-muted)',
            textTransform: 'uppercase',
            fontWeight: '600',
          }}
        >
          My Rooms
        </div>
        {myRooms.map(room => {
          const unreadCount = getUnreadCount(room.id);
          return (
            <div
              key={room.id.toString()}
              className={`room-item ${currentRoomId === room.id ? 'active' : ''}`}
              onClick={() => onRoomSelect(room.id)}
            >
              <span># {room.name}</span>
              {unreadCount > 0 && (
                <span className="unread-badge">{unreadCount}</span>
              )}
            </div>
          );
        })}
      </div>

      {/* Public Rooms */}
      <div className="room-list" style={{ flex: 1 }}>
        <div
          style={{
            padding: '8px 16px',
            fontSize: '12px',
            color: 'var(--text-muted)',
            textTransform: 'uppercase',
            fontWeight: '600',
          }}
        >
          Public Rooms
        </div>
        {rooms
          .filter(room => room.isPublic && !myRoomIds.has(room.id))
          .map(room => (
            <div
              key={room.id.toString()}
              className="room-item"
              onClick={() => handleJoinRoom(room.id)}
            >
              <span># {room.name}</span>
              <button className="btn" style={{ marginLeft: 'auto' }}>
                Join
              </button>
            </div>
          ))}
      </div>

      {/* Online Users */}
      <div className="user-list">
        <div
          style={{
            fontSize: '12px',
            color: 'var(--text-muted)',
            textTransform: 'uppercase',
            fontWeight: '600',
            marginBottom: '8px',
          }}
        >
          Online — {onlineUsers.length}
        </div>
        {onlineUsers.slice(0, 20).map(user => (
          <div key={user.identity.toHexString()} className="user-item">
            <div className="user-status online"></div>
            <span>{user.displayName}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
