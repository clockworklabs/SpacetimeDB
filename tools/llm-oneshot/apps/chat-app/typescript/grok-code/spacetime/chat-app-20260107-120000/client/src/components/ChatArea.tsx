import { useState, useEffect, useRef } from 'react';
import { useTable } from 'spacetimedb/react';
import { tables } from '../module_bindings';
import MessageItem from './MessageItem';
import MessageInput from './MessageInput';

interface ChatAreaProps {
  roomId: bigint | null;
  currentUser: any;
  users: readonly any[];
}

export default function ChatArea({
  roomId,
  currentUser,
  users,
}: ChatAreaProps) {
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [showScheduled, setShowScheduled] = useState(false);

  // Get data
  const [messages] = useTable(tables.message);
  const [messageEdits] = useTable(tables.messageEdit);
  const [reactions] = useTable(tables.messageReaction);
  const [readReceipts] = useTable(tables.readReceipt);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // Filter data for current room
  const roomMessages = roomId ? messages.filter(m => m.roomId === roomId) : [];
  const roomEdits = roomId
    ? messageEdits.filter(e => roomMessages.some(m => m.id === e.messageId))
    : [];
  const roomReactions = roomId
    ? reactions.filter(r => roomMessages.some(m => m.id === r.messageId))
    : [];
  const roomTypingIndicators = roomId
    ? typingIndicators.filter(t => t.roomId === roomId)
    : [];
  const myScheduledMessages = scheduledMessages.filter(
    s => s.authorId.toHexString() === currentUser?.identity.toHexString()
  );

  // Get room info
  const [rooms] = useTable(tables.room);
  const currentRoom = roomId ? rooms.find(r => r.id === roomId) : null;

  // Sort messages by creation time
  const sortedMessages = [...roomMessages].sort((a, b) =>
    Number(a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch)
  );

  // Get typing users
  const typingUsers = roomTypingIndicators
    .filter(t => t.userId.toHexString() !== currentUser?.identity.toHexString())
    .map(t => {
      const user = users.find(
        u => u.identity.toHexString() === t.userId.toHexString()
      );
      return user?.displayName || 'Unknown';
    })
    .filter(Boolean);

  // Scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [sortedMessages]);

  // Mark messages as read when viewing
  useEffect(() => {
    if (roomId && sortedMessages.length > 0 && window.__db_conn) {
      const latestMessage = sortedMessages[sortedMessages.length - 1];
      try {
        window.__db_conn.reducers.markMessageRead({
          messageId: latestMessage.id,
        });
      } catch (error) {
        console.error('Failed to mark message read:', error);
      }
    }
  }, [roomId, sortedMessages.length]);

  if (!roomId || !currentRoom) {
    return (
      <div className="main-content">
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100%',
            color: 'var(--text-muted)',
          }}
        >
          Select a room to start chatting
        </div>
      </div>
    );
  }

  return (
    <div className="main-content">
      {/* Chat Header */}
      <div className="chat-header">
        <span># {currentRoom.name}</span>
        <button
          onClick={() => setShowScheduled(!showScheduled)}
          className="btn"
          style={{ marginLeft: 'auto' }}
        >
          Scheduled ({myScheduledMessages.length})
        </button>
      </div>

      {/* Scheduled Messages Panel */}
      {showScheduled && (
        <div
          style={{
            backgroundColor: 'var(--bg-secondary)',
            borderBottom: '1px solid var(--border-color)',
            padding: '16px',
            maxHeight: '200px',
            overflowY: 'auto',
          }}
        >
          <h4 style={{ marginBottom: '8px' }}>Scheduled Messages</h4>
          {myScheduledMessages.length === 0 ? (
            <div style={{ color: 'var(--text-muted)', fontStyle: 'italic' }}>
              No scheduled messages
            </div>
          ) : (
            myScheduledMessages.map(msg => {
              const room = rooms.find(r => r.id === msg.roomId);
              return (
                <div
                  key={msg.scheduledId.toString()}
                  className="scheduled-message"
                >
                  <div style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                    #{room?.name} • Scheduled for{' '}
                    {msg.scheduledAt.tag === 'Time'
                      ? new Date(
                          Number(
                            msg.scheduledAt.value.microsSinceUnixEpoch / 1000n
                          )
                        ).toLocaleString()
                      : 'Unknown time'}
                  </div>
                  <div>{msg.content}</div>
                  <button
                    onClick={() => {
                      if (window.__db_conn) {
                        window.__db_conn.reducers.cancelScheduledMessage({
                          scheduledId: msg.scheduledId,
                        });
                      }
                    }}
                    className="btn"
                    style={{ marginTop: '4px', fontSize: '12px' }}
                  >
                    Cancel
                  </button>
                </div>
              );
            })
          )}
        </div>
      )}

      {/* Messages */}
      <div className="chat-messages">
        {sortedMessages.map(message => (
          <MessageItem
            key={message.id.toString()}
            message={message}
            edits={roomEdits.filter(e => e.messageId === message.id)}
            reactions={roomReactions.filter(r => r.messageId === message.id)}
            readReceipts={readReceipts.filter(r => r.messageId === message.id)}
            users={users}
            currentUser={currentUser}
            onEdit={async (messageId, newContent) => {
              if (window.__db_conn) {
                await window.__db_conn.reducers.editMessage({
                  messageId,
                  newContent,
                });
              }
            }}
            onReact={async (messageId, emoji) => {
              if (window.__db_conn) {
                await window.__db_conn.reducers.toggleReaction({
                  messageId,
                  emoji,
                });
              }
            }}
          />
        ))}
        <div ref={messagesEndRef} />
      </div>

      {/* Typing Indicator */}
      {typingUsers.length > 0 && (
        <div className="typing-indicator">
          {typingUsers.length === 1
            ? `${typingUsers[0]} is typing...`
            : `${typingUsers.slice(0, -1).join(', ')} and ${typingUsers[typingUsers.length - 1]} are typing...`}
        </div>
      )}

      {/* Message Input */}
      <MessageInput
        roomId={roomId}
        onSendMessage={async content => {
          if (window.__db_conn) {
            await window.__db_conn.reducers.sendMessage({ roomId, content });
          }
        }}
        onStartTyping={() => {
          if (window.__db_conn) {
            window.__db_conn.reducers.startTyping({ roomId });
          }
        }}
        onStopTyping={() => {
          if (window.__db_conn) {
            window.__db_conn.reducers.stopTyping({ roomId });
          }
        }}
      />
    </div>
  );
}
