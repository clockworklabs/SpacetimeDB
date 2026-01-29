import React from 'react';
import { Room, UnreadCount } from '../types';

interface RoomListProps {
  rooms: Room[];
  currentRoomId: string | null;
  unreadCounts: UnreadCount[];
  onJoinRoom: (roomId: string) => void;
}

function RoomList({
  rooms,
  currentRoomId,
  unreadCounts,
  onJoinRoom,
}: RoomListProps) {
  const getUnreadCount = (roomId: string) => {
    const count = unreadCounts.find(uc => uc.roomId === roomId);
    return count?.count || 0;
  };

  return (
    <div
      style={{
        flex: 1,
        overflowY: 'auto',
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
        Rooms
      </h4>

      {rooms.map(room => {
        const unreadCount = getUnreadCount(room.id);
        const isActive = room.id === currentRoomId;

        return (
          <div
            key={room.id}
            onClick={() => onJoinRoom(room.id)}
            style={{
              padding: '0.75rem 1rem',
              marginBottom: '0.25rem',
              background: isActive ? 'var(--bg-hover)' : 'transparent',
              borderRadius: '4px',
              cursor: 'pointer',
              border: isActive
                ? '1px solid var(--accent)'
                : '1px solid transparent',
              position: 'relative',
            }}
          >
            <div
              style={{
                fontWeight: isActive ? 'bold' : 'normal',
                color: isActive ? 'var(--accent)' : 'var(--text-primary)',
              }}
            >
              {room.name}
            </div>

            <div
              style={{
                fontSize: '0.8rem',
                color: 'var(--text-secondary)',
                marginTop: '0.25rem',
              }}
            >
              {room.memberCount} member{room.memberCount !== 1 ? 's' : ''}
            </div>

            {unreadCount > 0 && (
              <div
                style={{
                  position: 'absolute',
                  top: '0.5rem',
                  right: '0.5rem',
                  background: 'var(--error)',
                  color: 'white',
                  borderRadius: '10px',
                  padding: '0.2rem 0.5rem',
                  fontSize: '0.7rem',
                  fontWeight: 'bold',
                  minWidth: '18px',
                  textAlign: 'center',
                }}
              >
                {unreadCount > 99 ? '99+' : unreadCount}
              </div>
            )}
          </div>
        );
      })}

      {rooms.length === 0 && (
        <div
          style={{
            padding: '1rem',
            color: 'var(--text-muted)',
            textAlign: 'center',
            fontSize: '0.9rem',
          }}
        >
          No rooms yet. Create one to get started!
        </div>
      )}
    </div>
  );
}

export default RoomList;
