import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { useTable } from 'spacetimedb/react';
import { DbConnection, tables, reducers } from './module_bindings';
import { MODULE_NAME } from './config';

declare global {
  interface Window {
    __db_conn: DbConnection | null;
    __my_identity: any;
  }
}

function App() {
  const [currentRoomId, setCurrentRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [roomNameInput, setRoomNameInput] = useState('');
  const [userNameInput, setUserNameInput] = useState('');
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editInput, setEditInput] = useState('');
  const [showEditHistory, setShowEditHistory] = useState<bigint | null>(null);
  const [scheduledDelay, setScheduledDelay] = useState(60);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [isScheduled, setIsScheduled] = useState(false);

  const conn = window.__db_conn;
  const myIdentity = window.__my_identity;

  // Subscribe to all data
  const users = useTable(tables.user);
  const rooms = useTable(tables.room);
  const roomMembers = useTable(tables.room_member);
  const messages = useTable(tables.message);
  const messageEdits = useTable(tables.message_edit);
  const reactions = useTable(tables.reaction);
  const readReceipts = useTable(tables.read_receipt);
  const roomReadPositions = useTable(tables.room_read_position);
  const typingIndicators = useTable(tables.typing_indicator);
  const scheduledMessages = useTable(tables.scheduled_message);

  // Connection and subscription setup
  useEffect(() => {
    if (!conn) return;

    // Subscribe to all tables
    conn
      .subscriptionBuilder()
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM message_edit',
        'SELECT * FROM reaction',
        'SELECT * FROM read_receipt',
        'SELECT * FROM room_read_position',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM scheduled_message',
      ])
      .run();
  }, [conn]);

  // Set user name on connect
  const handleSetUserName = useCallback(() => {
    if (!conn || !userNameInput.trim()) return;
    conn.reducers.set_name({ name: userNameInput.trim() });
    setUserNameInput('');
  }, [conn, userNameInput]);

  // Create room
  const handleCreateRoom = useCallback(() => {
    if (!conn || !roomNameInput.trim()) return;
    conn.reducers.create_room({
      name: roomNameInput.trim(),
      description: '',
    });
    setRoomNameInput('');
  }, [conn, roomNameInput]);

  // Join room
  const handleJoinRoom = useCallback(
    (roomId: bigint) => {
      if (!conn) return;
      conn.reducers.join_room({ roomId });
    },
    [conn]
  );

  // Send message
  const handleSendMessage = useCallback(() => {
    if (!conn || !messageInput.trim() || !currentRoomId) return;

    if (isScheduled) {
      conn.reducers.schedule_message({
        roomId: currentRoomId,
        content: messageInput.trim(),
        delaySeconds: BigInt(scheduledDelay),
      });
    } else if (isEphemeral) {
      conn.reducers.send_ephemeral_message({
        roomId: currentRoomId,
        content: messageInput.trim(),
        durationSeconds: BigInt(ephemeralDuration),
      });
    } else {
      conn.reducers.send_message({
        roomId: currentRoomId,
        content: messageInput.trim(),
      });
    }

    setMessageInput('');
    setIsScheduled(false);
    setIsEphemeral(false);
  }, [
    conn,
    messageInput,
    currentRoomId,
    isScheduled,
    isEphemeral,
    scheduledDelay,
    ephemeralDuration,
  ]);

  // Edit message
  const handleEditMessage = useCallback(() => {
    if (!conn || !editingMessageId || !editInput.trim()) return;
    conn.reducers.edit_message({
      messageId: editingMessageId,
      newContent: editInput.trim(),
    });
    setEditingMessageId(null);
    setEditInput('');
  }, [conn, editingMessageId, editInput]);

  // Start editing
  const startEditing = useCallback((messageId: bigint, content: string) => {
    setEditingMessageId(messageId);
    setEditInput(content);
  }, []);

  // Cancel editing
  const cancelEditing = useCallback(() => {
    setEditingMessageId(null);
    setEditInput('');
  }, []);

  // Toggle reaction
  const handleToggleReaction = useCallback(
    (messageId: bigint, emoji: string) => {
      if (!conn) return;
      conn.reducers.toggle_reaction({ messageId, emoji });
    },
    [conn]
  );

  // Mark message as read
  const handleMarkAsRead = useCallback(
    (messageId: bigint) => {
      if (!conn) return;
      conn.reducers.mark_message_read({ messageId });
    },
    [conn]
  );

  // Mark room as read
  const handleMarkRoomRead = useCallback(
    (roomId: bigint) => {
      if (!conn) return;
      conn.reducers.mark_room_read({ roomId });
    },
    [conn]
  );

  // Typing handlers
  const handleTypingStart = useCallback(() => {
    if (!conn || !currentRoomId) return;
    conn.reducers.start_typing({ roomId: currentRoomId });
  }, [conn, currentRoomId]);

  const handleTypingStop = useCallback(() => {
    if (!conn || !currentRoomId) return;
    conn.reducers.stop_typing({ roomId: currentRoomId });
  }, [conn, currentRoomId]);

  // Cancel scheduled message
  const handleCancelScheduled = useCallback(
    (scheduledId: bigint) => {
      if (!conn) return;
      conn.reducers.cancel_scheduled_message({ scheduledId });
    },
    [conn]
  );

  // Get current user
  const currentUser = users.find(
    u => u.identity.toHexString() === myIdentity?.toHexString()
  );

  // Get current room messages
  const currentRoomMessages = messages.filter(m => m.roomId === currentRoomId);

  // Get current room members
  const currentRoomMembers = roomMembers.filter(
    m => m.roomId === currentRoomId
  );

  // Get current room typing indicators
  const currentRoomTyping = typingIndicators.filter(
    t => t.roomId === currentRoomId
  );

  // Get unread counts for rooms
  const getUnreadCount = useCallback(
    (roomId: bigint) => {
      const readPos = roomReadPositions.find(
        rp =>
          rp.roomId === roomId &&
          rp.userId.toHexString() === myIdentity?.toHexString()
      );
      const lastReadId = readPos?.lastReadMessageId || 0n;

      const roomMessages = messages.filter(m => m.roomId === roomId);
      return roomMessages.filter(m => m.id > lastReadId).length;
    },
    [messages, roomReadPositions, myIdentity]
  );

  // Get reactions for message
  const getMessageReactions = useCallback(
    (messageId: bigint) => {
      const messageReactions = reactions.filter(r => r.messageId === messageId);
      const grouped = messageReactions.reduce(
        (acc, reaction) => {
          if (!acc[reaction.emoji]) {
            acc[reaction.emoji] = { count: 0, users: [], hasReacted: false };
          }
          acc[reaction.emoji].count++;
          acc[reaction.emoji].users.push(reaction.userId);
          if (reaction.userId.toHexString() === myIdentity?.toHexString()) {
            acc[reaction.emoji].hasReacted = true;
          }
          return acc;
        },
        {} as Record<
          string,
          { count: number; users: any[]; hasReacted: boolean }
        >
      );
      return grouped;
    },
    [reactions, myIdentity]
  );

  // Get read receipts for message
  const getReadReceipts = useCallback(
    (messageId: bigint) => {
      return readReceipts.filter(r => r.messageId === messageId);
    },
    [readReceipts]
  );

  // Get edit history for message
  const getEditHistory = useCallback(
    (messageId: bigint) => {
      return messageEdits
        .filter(e => e.messageId === messageId)
        .sort((a, b) =>
          Number(
            a.editedAt.microsSinceUnixEpoch - b.editedAt.microsSinceUnixEpoch
          )
        );
    },
    [messageEdits]
  );

  // Get user name
  const getUserName = useCallback(
    (identity: any) => {
      const user = users.find(
        u => u.identity.toHexString() === identity.toHexString()
      );
      return user?.name || 'Unknown';
    },
    [users]
  );

  if (!conn || !myIdentity) {
    return (
      <div className="app">
        <div className="messages-container">
          <p>Connecting to SpacetimeDB...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        {/* User info */}
        <div
          style={{
            padding: '16px',
            borderBottom: '1px solid var(--border-color)',
          }}
        >
          {!currentUser?.name && (
            <div style={{ marginBottom: '8px' }}>
              <input
                className="input"
                placeholder="Enter your name"
                value={userNameInput}
                onChange={e => setUserNameInput(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSetUserName()}
                style={{ width: '100%', marginBottom: '4px' }}
              />
              <button
                className="button"
                onClick={handleSetUserName}
                style={{ width: '100%' }}
              >
                Set Name
              </button>
            </div>
          )}
          {currentUser && (
            <div>
              <div style={{ fontWeight: '600', marginBottom: '4px' }}>
                {currentUser.name}
              </div>
              <div style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                {currentUser.online ? '🟢 Online' : '⚫ Offline'}
              </div>
            </div>
          )}
        </div>

        {/* Create room */}
        <div
          style={{
            padding: '16px',
            borderBottom: '1px solid var(--border-color)',
          }}
        >
          <input
            className="input"
            placeholder="Room name"
            value={roomNameInput}
            onChange={e => setRoomNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
            style={{ width: '100%', marginBottom: '4px' }}
          />
          <button
            className="button"
            onClick={handleCreateRoom}
            style={{ width: '100%' }}
          >
            Create Room
          </button>
        </div>

        {/* Room list */}
        <div style={{ flex: 1, overflowY: 'auto' }}>
          {rooms.map(room => {
            const unreadCount = getUnreadCount(room.id);
            const isActive = currentRoomId === room.id;

            return (
              <div
                key={room.id.toString()}
                className={`room-item ${isActive ? 'active' : ''}`}
                onClick={() => setCurrentRoomId(room.id)}
              >
                <span className="room-name">#{room.name}</span>
                {unreadCount > 0 && (
                  <span className="unread-badge">{unreadCount}</span>
                )}
              </div>
            );
          })}
        </div>

        {/* Users online */}
        <div
          style={{
            padding: '16px',
            borderTop: '1px solid var(--border-color)',
          }}
        >
          <div
            style={{
              fontSize: '12px',
              color: 'var(--text-muted)',
              marginBottom: '8px',
            }}
          >
            USERS ONLINE — {users.filter(u => u.online).length}
          </div>
          {users
            .filter(u => u.online)
            .map(user => (
              <div key={user.id.toString()} className="user-item">
                <div
                  className={`user-status ${user.online ? 'online' : 'offline'}`}
                ></div>
                <span>{user.name}</span>
              </div>
            ))}
        </div>
      </div>

      {/* Main content */}
      <div className="main-content">
        {/* Chat header */}
        <div className="chat-header">
          {currentRoomId ? (
            <>
              #{rooms.find(r => r.id === currentRoomId)?.name}
              <button
                className="button"
                onClick={() => handleMarkRoomRead(currentRoomId)}
                style={{
                  marginLeft: 'auto',
                  fontSize: '12px',
                  padding: '4px 8px',
                }}
              >
                Mark Read
              </button>
            </>
          ) : (
            'Select a room'
          )}
        </div>

        {/* Messages */}
        <div className="messages-container">
          {currentRoomId ? (
            <>
              {currentRoomMessages
                .sort((a, b) =>
                  Number(
                    a.sentAt.microsSinceUnixEpoch -
                      b.sentAt.microsSinceUnixEpoch
                  )
                )
                .map(message => {
                  const isOwnMessage =
                    message.senderId.toHexString() === myIdentity.toHexString();
                  const reactions = getMessageReactions(message.id);
                  const readReceipts = getReadReceipts(message.id);
                  const editHistory = getEditHistory(message.id);

                  return (
                    <div key={message.id.toString()} className="message">
                      <div className="message-avatar"></div>
                      <div className="message-content">
                        <div className="message-header">
                          <span className="message-author">
                            {message.senderName}
                          </span>
                          <span className="message-timestamp">
                            {new Date(
                              Number(
                                message.sentAt.microsSinceUnixEpoch / 1000n
                              )
                            ).toLocaleTimeString()}
                          </span>
                          {message.editedAt && (
                            <span className="message-edited">(edited)</span>
                          )}
                          {message.isEphemeral && (
                            <span className="ephemeral-countdown">
                              (disappears in{' '}
                              {Math.max(
                                0,
                                Math.ceil(
                                  (Number(
                                    message.ephemeralExpiresAt
                                      ?.microsSinceUnixEpoch || 0n
                                  ) -
                                    Date.now() * 1000) /
                                    1000000
                                )
                              )}
                              s)
                            </span>
                          )}
                        </div>
                        <div className="message-text">
                          {editingMessageId === message.id ? (
                            <div>
                              <input
                                className="input"
                                value={editInput}
                                onChange={e => setEditInput(e.target.value)}
                                onKeyDown={e => {
                                  if (e.key === 'Enter') handleEditMessage();
                                  if (e.key === 'Escape') cancelEditing();
                                }}
                                style={{ width: '100%', marginBottom: '4px' }}
                              />
                              <button
                                className="button"
                                onClick={handleEditMessage}
                                style={{ marginRight: '4px' }}
                              >
                                Save
                              </button>
                              <button
                                className="button"
                                onClick={cancelEditing}
                                style={{
                                  backgroundColor: 'var(--error-color)',
                                }}
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            message.content
                          )}
                        </div>

                        {/* Reactions */}
                        {Object.keys(reactions).length > 0 && (
                          <div className="reactions">
                            {Object.entries(reactions).map(([emoji, data]) => (
                              <div
                                key={emoji}
                                className={`reaction ${data.hasReacted ? 'active' : ''}`}
                                onClick={() =>
                                  handleToggleReaction(message.id, emoji)
                                }
                              >
                                {emoji} {data.count}
                              </div>
                            ))}
                          </div>
                        )}

                        {/* Edit history */}
                        {editHistory.length > 0 &&
                          showEditHistory === message.id && (
                            <div className="edit-history">
                              {editHistory.map(edit => (
                                <div
                                  key={edit.id.toString()}
                                  className="edit-entry"
                                >
                                  <strong>{getUserName(edit.editedBy)}</strong>{' '}
                                  edited at{' '}
                                  {new Date(
                                    Number(
                                      edit.editedAt.microsSinceUnixEpoch / 1000n
                                    )
                                  ).toLocaleTimeString()}
                                  <br />
                                  <del style={{ color: 'var(--error-color)' }}>
                                    {edit.previousContent}
                                  </del>
                                  {' → '}
                                  <ins
                                    style={{ color: 'var(--success-color)' }}
                                  >
                                    {edit.newContent}
                                  </ins>
                                </div>
                              ))}
                            </div>
                          )}

                        {/* Message actions */}
                        {isOwnMessage && editingMessageId !== message.id && (
                          <div style={{ marginTop: '4px' }}>
                            <button
                              className="button"
                              onClick={() =>
                                startEditing(message.id, message.content)
                              }
                              style={{
                                fontSize: '11px',
                                padding: '2px 6px',
                                marginRight: '4px',
                              }}
                            >
                              Edit
                            </button>
                            {editHistory.length > 0 && (
                              <button
                                className="button"
                                onClick={() =>
                                  setShowEditHistory(
                                    showEditHistory === message.id
                                      ? null
                                      : message.id
                                  )
                                }
                                style={{ fontSize: '11px', padding: '2px 6px' }}
                              >
                                {showEditHistory === message.id
                                  ? 'Hide'
                                  : 'Show'}{' '}
                                History
                              </button>
                            )}
                          </div>
                        )}

                        {/* Add reaction */}
                        <div style={{ marginTop: '4px' }}>
                          {['👍', '❤️', '😂', '😮', '😢'].map(emoji => (
                            <button
                              key={emoji}
                              className="button"
                              onClick={() =>
                                handleToggleReaction(message.id, emoji)
                              }
                              style={{
                                fontSize: '12px',
                                padding: '2px 4px',
                                marginRight: '2px',
                              }}
                            >
                              {emoji}
                            </button>
                          ))}
                        </div>

                        {/* Read receipts */}
                        {readReceipts.length > 0 && (
                          <div className="read-receipts">
                            Seen by{' '}
                            {readReceipts
                              .map(r => getUserName(r.userId))
                              .join(', ')}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}

              {/* Typing indicators */}
              {currentRoomTyping.length > 0 && (
                <div className="typing-indicator">
                  {currentRoomTyping.length === 1
                    ? `${currentRoomTyping[0].userName} is typing...`
                    : `${currentRoomTyping.length} people are typing...`}
                </div>
              )}
            </>
          ) : (
            <p>Select a room to start chatting</p>
          )}
        </div>

        {/* Message input */}
        {currentRoomId && (
          <div className="message-input-container">
            {/* Scheduled/Ephemeral options */}
            <div
              style={{
                marginBottom: '8px',
                display: 'flex',
                gap: '8px',
                alignItems: 'center',
              }}
            >
              <label>
                <input
                  type="checkbox"
                  checked={isScheduled}
                  onChange={e => {
                    setIsScheduled(e.target.checked);
                    if (e.target.checked) setIsEphemeral(false);
                  }}
                />
                Schedule
              </label>
              {isScheduled && (
                <input
                  type="number"
                  min="10"
                  max="86400"
                  value={scheduledDelay}
                  onChange={e => setScheduledDelay(Number(e.target.value))}
                  style={{ width: '80px' }}
                  className="input"
                />
              )}
              <label>
                <input
                  type="checkbox"
                  checked={isEphemeral}
                  onChange={e => {
                    setIsEphemeral(e.target.checked);
                    if (e.target.checked) setIsScheduled(false);
                  }}
                />
                Ephemeral
              </label>
              {isEphemeral && (
                <input
                  type="number"
                  min="10"
                  max="3600"
                  value={ephemeralDuration}
                  onChange={e => setEphemeralDuration(Number(e.target.value))}
                  style={{ width: '80px' }}
                  className="input"
                />
              )}
            </div>

            <textarea
              className="message-input"
              placeholder="Type a message..."
              value={messageInput}
              onChange={e => {
                setMessageInput(e.target.value);
                handleTypingStart();
              }}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  handleSendMessage();
                }
              }}
              onBlur={handleTypingStop}
            />
          </div>
        )}

        {/* Scheduled messages */}
        {currentRoomId &&
          scheduledMessages.filter(sm => sm.roomId === currentRoomId).length >
            0 && (
            <div
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--bg-secondary)',
                borderTop: '1px solid var(--border-color)',
              }}
            >
              <div
                style={{
                  fontSize: '12px',
                  color: 'var(--text-muted)',
                  marginBottom: '4px',
                }}
              >
                Scheduled Messages:
              </div>
              {scheduledMessages
                .filter(sm => sm.roomId === currentRoomId)
                .map(sm => (
                  <div
                    key={sm.scheduledId.toString()}
                    style={{ fontSize: '12px', marginBottom: '2px' }}
                  >
                    "{sm.content}" (in{' '}
                    {Math.ceil(
                      (Number(sm.scheduledAt.value.microsSinceUnixEpoch) -
                        Date.now() * 1000) /
                        1000000
                    )}
                    s)
                    <button
                      className="button"
                      onClick={() => handleCancelScheduled(sm.scheduledId)}
                      style={{
                        fontSize: '10px',
                        padding: '1px 4px',
                        marginLeft: '4px',
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                ))}
            </div>
          )}
      </div>
    </div>
  );
}

export default App;
