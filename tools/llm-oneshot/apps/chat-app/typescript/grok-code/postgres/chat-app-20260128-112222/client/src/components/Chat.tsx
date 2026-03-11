import React, { useState, useEffect, useRef } from 'react';
import {
  User,
  Room,
  Message,
  OnlineUser,
  UnreadCount,
  TypingUser,
} from '../types';
import RoomList from './RoomList';
import MessageList from './MessageList';
import MessageInput from './MessageInput';
import OnlineUsers from './OnlineUsers';
import CreateRoomModal from './CreateRoomModal';

interface ChatProps {
  currentUser: User;
  rooms: Room[];
  currentRoomId: string | null;
  messages: Message[];
  onlineUsers: OnlineUser[];
  unreadCounts: UnreadCount[];
  typingUsers: { [roomId: string]: TypingUser[] };
  onCreateRoom: (name: string) => void;
  onJoinRoom: (roomId: string) => void;
  onSendMessage: (
    roomId: string,
    content: string,
    scheduledFor?: Date,
    expiresAt?: Date
  ) => void;
  onEditMessage: (messageId: string, content: string) => void;
  onStartTyping: (roomId: string) => void;
  onStopTyping: (roomId: string) => void;
  onMarkAsRead: (roomId: string, messageId: string) => void;
  onAddReaction: (messageId: string, emoji: string) => void;
  onRemoveReaction: (messageId: string, emoji: string) => void;
}

function Chat({
  currentUser,
  rooms,
  currentRoomId,
  messages,
  onlineUsers,
  unreadCounts,
  typingUsers,
  onCreateRoom,
  onJoinRoom,
  onSendMessage,
  onEditMessage,
  onStartTyping,
  onStopTyping,
  onMarkAsRead,
  onAddReaction,
  onRemoveReaction,
}: ChatProps) {
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (currentRoomId && messages.length > 0) {
      const lastMessage = messages[messages.length - 1];
      onMarkAsRead(currentRoomId, lastMessage.id);
    }
  }, [currentRoomId, messages, onMarkAsRead]);

  const currentRoom = rooms.find(room => room.id === currentRoomId);
  const currentTypingUsers = currentRoomId
    ? typingUsers[currentRoomId] || []
    : [];

  return (
    <div
      style={{
        display: 'flex',
        height: '100vh',
        background: 'var(--bg-primary)',
      }}
    >
      {/* Sidebar */}
      <div
        style={{
          width: '300px',
          borderRight: '1px solid var(--border)',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* User info */}
        <div
          style={{
            padding: '1rem',
            borderBottom: '1px solid var(--border)',
            background: 'var(--bg-secondary)',
          }}
        >
          <h3 style={{ color: 'var(--accent)', marginBottom: '0.5rem' }}>
            {currentUser.displayName}
          </h3>
          <button
            onClick={() => setShowCreateRoom(true)}
            style={{
              padding: '0.5rem 1rem',
              background: 'var(--accent)',
              border: 'none',
              borderRadius: '4px',
              color: 'var(--bg-primary)',
              cursor: 'pointer',
              fontSize: '0.9rem',
            }}
          >
            Create Room
          </button>
        </div>

        {/* Room list */}
        <RoomList
          rooms={rooms}
          currentRoomId={currentRoomId}
          unreadCounts={unreadCounts}
          onJoinRoom={onJoinRoom}
        />

        {/* Online users */}
        <OnlineUsers onlineUsers={onlineUsers} />
      </div>

      {/* Main chat area */}
      <div
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {/* Room header */}
        {currentRoom && (
          <div
            style={{
              padding: '1rem',
              borderBottom: '1px solid var(--border)',
              background: 'var(--bg-secondary)',
            }}
          >
            <h2 style={{ color: 'var(--accent)', margin: 0 }}>
              {currentRoom.name}
            </h2>
            <div style={{ color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
              {currentRoom.memberCount} member
              {currentRoom.memberCount !== 1 ? 's' : ''}
            </div>
          </div>
        )}

        {/* Messages */}
        <div
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '1rem',
          }}
        >
          {currentRoomId ? (
            <>
              <MessageList
                messages={messages}
                currentUser={currentUser}
                onEditMessage={onEditMessage}
                onAddReaction={onAddReaction}
                onRemoveReaction={onRemoveReaction}
              />
              <div ref={messagesEndRef} />

              {/* Typing indicators */}
              {currentTypingUsers.length > 0 && (
                <div
                  style={{
                    marginTop: '1rem',
                    color: 'var(--text-secondary)',
                    fontSize: '0.9rem',
                    fontStyle: 'italic',
                  }}
                >
                  {currentTypingUsers.length === 1
                    ? `${currentTypingUsers[0].displayName} is typing...`
                    : `${currentTypingUsers.length} users are typing...`}
                </div>
              )}
            </>
          ) : (
            <div
              style={{
                display: 'flex',
                justifyContent: 'center',
                alignItems: 'center',
                height: '100%',
                color: 'var(--text-secondary)',
              }}
            >
              Select a room to start chatting
            </div>
          )}
        </div>

        {/* Message input */}
        {currentRoomId && (
          <MessageInput
            roomId={currentRoomId}
            onSendMessage={onSendMessage}
            onStartTyping={onStartTyping}
            onStopTyping={onStopTyping}
          />
        )}
      </div>

      {/* Create room modal */}
      {showCreateRoom && (
        <CreateRoomModal
          onCreateRoom={onCreateRoom}
          onClose={() => setShowCreateRoom(false)}
        />
      )}
    </div>
  );
}

export default Chat;
