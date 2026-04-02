import { useState, useEffect, useRef, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, MessageEditHistory } from './module_bindings/types';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function timeAgo(date: Date): string {
  const secs = Math.floor((Date.now() - date.getTime()) / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function statusColor(status: string, online: boolean): string {
  if (!online || status === 'invisible') return '#6f7987';
  if (status === 'away') return '#fbdc8e';
  if (status === 'dnd') return '#ff4c4c';
  return '#4cf490';
}

function idStr(id: { toHexString(): string }): string {
  return id.toHexString();
}

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;
  const [subscribed, setSubscribed] = useState(false);

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe to all tables
  useEffect(() => {
    if (!conn || !isActive) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM room_ban',
        'SELECT * FROM message',
        'SELECT * FROM message_edit_history',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM message_reaction',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  // Heartbeat to update lastActive
  useEffect(() => {
    if (!conn || !isActive || !subscribed) return;
    const iv = setInterval(() => conn.reducers.heartbeat({}), 30_000);
    return () => clearInterval(iv);
  }, [conn, isActive, subscribed]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [editHistories] = useTable(tables.messageEditHistory);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [reactions] = useTable(tables.messageReaction);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // Local state
  const [nameInput, setNameInput] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [newRoomInput, setNewRoomInput] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [showMembers, setShowMembers] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editInput, setEditInput] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<bigint | null>(null);
  const [showScheduled, setShowScheduled] = useState(false);
  const [now, setNow] = useState(Date.now());

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Clock tick for timers
  useEffect(() => {
    const iv = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(iv);
  }, []);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, selectedRoomId]);

  const myUser = myIdentity ? users.find((u) => idStr(u.identity) === idStr(myIdentity)) : null;

  // Mark room as read when entering it or receiving new messages
  useEffect(() => {
    if (!conn || !selectedRoomId || !myIdentity) return;
    const roomMessages = messages.filter((m) => m.roomId === selectedRoomId);
    if (roomMessages.length === 0) return;
    const lastMsg = roomMessages.reduce((a, b) => (a.id > b.id ? a : b));
    conn.reducers.markRoomRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [conn, selectedRoomId, messages.length, myIdentity]);

  // Unread counts per room
  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const receipt = readReceipts.find(
      (r) => r.roomId === roomId && idStr(r.userId) === idStr(myIdentity)
    );
    const lastReadId = receipt ? receipt.messageId : 0n;
    return messages.filter((m) => m.roomId === roomId && m.id > lastReadId).length;
  }

  // Typing indicator logic
  const sendTyping = useCallback(() => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.stopTyping({ roomId: selectedRoomId });
    }, 3000);
  }, [conn, selectedRoomId]);

  // Typing users in current room
  function getTypingUsers(): string[] {
    if (!selectedRoomId || !myIdentity) return [];
    const EXPIRE_MS = 5000;
    return typingIndicators
      .filter(
        (ti) =>
          ti.roomId === selectedRoomId &&
          idStr(ti.userId) !== idStr(myIdentity) &&
          now - Number(ti.lastTypingAt.microsSinceUnixEpoch / 1000n) < EXPIRE_MS
      )
      .map((ti) => {
        const u = users.find((u) => idStr(u.identity) === idStr(ti.userId));
        return u?.name ?? 'Someone';
      });
  }

  // Send message
  function handleSendMessage() {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    if (showSchedule && scheduleTime) {
      const sendAtMs = new Date(scheduleTime).getTime();
      conn.reducers.scheduleMessage({
        roomId: selectedRoomId,
        text: messageInput.trim(),
        sendAtMicros: BigInt(sendAtMs) * 1000n,
      });
      setShowSchedule(false);
      setScheduleTime('');
    } else {
      conn.reducers.sendMessage({
        roomId: selectedRoomId,
        text: messageInput.trim(),
        isEphemeral,
        durationSecs: isEphemeral ? ephemeralDuration : 0,
      });
    }
    setMessageInput('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    conn.reducers.stopTyping({ roomId: selectedRoomId });
  }

  // Registration screen
  if (!subscribed) {
    return (
      <div className="loading-screen">
        <div className="loading-spinner" />
        <p>Connecting to SpacetimeDB Chat...</p>
      </div>
    );
  }

  if (!myUser) {
    return (
      <div className="register-screen">
        <h1>SpacetimeDB Chat</h1>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (nameInput.trim() && conn) {
              conn.reducers.register({ name: nameInput.trim() });
            }
          }}
        >
          <input
            type="text"
            placeholder="Enter your name"
            value={nameInput}
            onChange={(e) => setNameInput(e.target.value)}
            maxLength={32}
            autoFocus
          />
          <button type="submit">Join</button>
        </form>
      </div>
    );
  }

  const selectedRoom = rooms.find((r) => r.id === selectedRoomId);
  const roomMessages = selectedRoomId
    ? messages.filter((m) => m.roomId === selectedRoomId).sort((a, b) => (a.id < b.id ? -1 : 1))
    : [];
  const myRoomIds = new Set(
    roomMembers.filter((rm) => idStr(rm.userId) === idStr(myIdentity!)).map((rm) => rm.roomId)
  );
  const myRooms = rooms.filter((r) => myRoomIds.has(r.id));
  const currentRoomMembers = selectedRoomId
    ? roomMembers.filter((rm) => rm.roomId === selectedRoomId)
    : [];
  const myMembership = selectedRoomId
    ? currentRoomMembers.find((rm) => idStr(rm.userId) === idStr(myIdentity!))
    : null;
  const typingUsers = getTypingUsers();

  // "Seen by" for a message: users whose readReceipt.messageId >= message.id, excluding self
  function getSeenBy(msg: Message): string[] {
    return readReceipts
      .filter(
        (r) =>
          r.roomId === msg.roomId &&
          r.messageId >= msg.id &&
          idStr(r.userId) !== idStr(myIdentity!)
      )
      .map((r) => {
        const u = users.find((u) => idStr(u.identity) === idStr(r.userId));
        return u?.name ?? '?';
      });
  }

  // Reactions for a message
  function getReactions(msgId: bigint): Map<string, { count: number; users: string[] }> {
    const map = new Map<string, { count: number; users: string[] }>();
    reactions
      .filter((r) => r.messageId === msgId)
      .forEach((r) => {
        const existing = map.get(r.emoji) ?? { count: 0, users: [] };
        const uName = users.find((u) => idStr(u.identity) === idStr(r.userId))?.name ?? '?';
        map.set(r.emoji, { count: existing.count + 1, users: [...existing.users, uName] });
      });
    return map;
  }

  // My scheduled messages for current room
  const myScheduled = scheduledMessages.filter(
    (s) => s.roomId === selectedRoomId && idStr(s.sender) === idStr(myIdentity!)
  );

  // Edit history modal content
  const historyEntries: MessageEditHistory[] = historyMessageId
    ? editHistories.filter((h) => h.messageId === historyMessageId).sort((a, b) => (a.id < b.id ? -1 : 1))
    : [];

  return (
    <div className="app">
      {/* Header */}
      <header className="app-header">
        <h1>SpacetimeDB Chat</h1>
        <div className="header-right">
          <span className="username">{myUser.name}</span>
          <select
            className="status-select"
            value={myUser.status}
            onChange={(e) => conn?.reducers.setStatus({ status: e.target.value })}
            title="Set status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
          <span
            className="status-dot"
            style={{ background: statusColor(myUser.status, myUser.online) }}
            title={myUser.status}
          />
        </div>
      </header>

      <div className="app-body">
        {/* Sidebar: Room List */}
        <aside className="sidebar">
          <div className="sidebar-header">
            <span>Rooms</span>
            <button
              className="btn-icon"
              onClick={() => setShowCreateRoom(!showCreateRoom)}
              title="Create Room"
            >
              +
            </button>
          </div>

          {showCreateRoom && (
            <form
              className="create-room-form"
              onSubmit={(e) => {
                e.preventDefault();
                if (newRoomInput.trim() && conn) {
                  conn.reducers.createRoom({ name: newRoomInput.trim() });
                  setNewRoomInput('');
                  setShowCreateRoom(false);
                }
              }}
            >
              <input
                type="text"
                placeholder="Room name"
                value={newRoomInput}
                onChange={(e) => setNewRoomInput(e.target.value)}
                autoFocus
                maxLength={64}
              />
              <button type="submit">Create</button>
            </form>
          )}

          <ul className="room-list">
            {/* Rooms I'm in */}
            {myRooms.map((room) => {
              const unread = getUnreadCount(room.id);
              const isSelected = room.id === selectedRoomId;
              return (
                <li
                  key={String(room.id)}
                  className={`room-item ${isSelected ? 'selected' : ''}`}
                  onClick={() => {
                    setSelectedRoomId(room.id);
                    setShowMembers(false);
                  }}
                >
                  <span className="room-name">{room.name}</span>
                  {unread > 0 && <span className="unread-badge">{unread}</span>}
                </li>
              );
            })}
          </ul>

          {/* Other rooms (not joined) */}
          <div className="sidebar-section-title">Discover</div>
          <ul className="room-list room-list-discover">
            {rooms
              .filter((r) => !myRoomIds.has(r.id))
              .map((room) => (
                <li key={String(room.id)} className="room-item">
                  <span className="room-name">{room.name}</span>
                  <button
                    className="btn-sm"
                    onClick={() => {
                      conn?.reducers.joinRoom({ roomId: room.id });
                      setSelectedRoomId(room.id);
                    }}
                  >
                    Join
                  </button>
                </li>
              ))}
          </ul>
        </aside>

        {/* Main Chat Area */}
        <main className="chat-main">
          {selectedRoom ? (
            <>
              {/* Room Header */}
              <div className="room-header">
                <span className="room-title">{selectedRoom.name}</span>
                <div className="room-header-actions">
                  <button
                    className="btn-sm"
                    onClick={() => setShowScheduled(!showScheduled)}
                  >
                    Scheduled
                  </button>
                  <button
                    className="btn-sm"
                    onClick={() => setShowMembers(!showMembers)}
                  >
                    Members
                  </button>
                  <button
                    className="btn-sm btn-danger"
                    onClick={() => {
                      conn?.reducers.leaveRoom({ roomId: selectedRoomId! });
                      setSelectedRoomId(null);
                    }}
                  >
                    Leave
                  </button>
                </div>
              </div>

              {/* Pending Scheduled Messages */}
              {showScheduled && myScheduled.length > 0 && (
                <div className="scheduled-panel">
                  <div className="panel-title">Pending Scheduled</div>
                  {myScheduled.map((s) => (
                    <div key={String(s.scheduledId)} className="scheduled-item">
                      <span className="scheduled-text">{s.text}</span>
                      <button
                        className="btn-sm btn-danger"
                        onClick={() =>
                          conn?.reducers.cancelScheduledMessage({ scheduledId: s.scheduledId })
                        }
                      >
                        Cancel
                      </button>
                    </div>
                  ))}
                </div>
              )}

              {/* Messages */}
              <div className="messages-area">
                {roomMessages.map((msg) => {
                  const sender = users.find((u) => idStr(u.identity) === idStr(msg.sender));
                  const isMe = idStr(msg.sender) === idStr(myIdentity!);
                  const seenBy = getSeenBy(msg);
                  const msgReactions = getReactions(msg.id);
                  const isEditing = editingMessageId === msg.id;
                  const isEdited = msg.editedAt !== undefined && msg.editedAt !== null;
                  const expiresInSecs = msg.expiresAt
                    ? Math.max(
                        0,
                        Math.floor(
                          (Number(msg.expiresAt.microsSinceUnixEpoch / 1000n) - now) / 1000
                        )
                      )
                    : null;

                  return (
                    <div key={String(msg.id)} className={`message ${isMe ? 'message-mine' : ''}`}>
                      <div className="message-meta">
                        <span className="message-sender">{sender?.name ?? '?'}</span>
                        <span className="message-time">
                          {tsToDate(msg.sentAt).toLocaleTimeString()}
                        </span>
                        {msg.isEphemeral && expiresInSecs !== null && (
                          <span className="ephemeral-badge" title="Ephemeral message">
                            expires {expiresInSecs}s
                          </span>
                        )}
                      </div>

                      {isEditing ? (
                        <div className="edit-form">
                          <input
                            type="text"
                            value={editInput}
                            onChange={(e) => setEditInput(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') {
                                conn?.reducers.editMessage({
                                  messageId: msg.id,
                                  newText: editInput,
                                });
                                setEditingMessageId(null);
                              } else if (e.key === 'Escape') {
                                setEditingMessageId(null);
                              }
                            }}
                            autoFocus
                          />
                          <button
                            className="btn-sm"
                            onClick={() => {
                              conn?.reducers.editMessage({ messageId: msg.id, newText: editInput });
                              setEditingMessageId(null);
                            }}
                          >
                            Save
                          </button>
                          <button className="btn-sm" onClick={() => setEditingMessageId(null)}>
                            Cancel
                          </button>
                        </div>
                      ) : (
                        <div className="message-content">
                          <span className="message-text">{msg.text}</span>
                          {isEdited && (
                            <span
                              className="edited-indicator"
                              onClick={() => setHistoryMessageId(msg.id)}
                              title="Click to view edit history"
                            >
                              (edited)
                            </span>
                          )}
                          {isMe && (
                            <button
                              className="btn-edit"
                              onClick={() => {
                                setEditingMessageId(msg.id);
                                setEditInput(msg.text);
                              }}
                            >
                              Edit
                            </button>
                          )}
                        </div>
                      )}

                      {/* Reactions */}
                      <div className="message-reactions">
                        {Array.from(msgReactions.entries()).map(([emoji, data]) => {
                          const myReaction = reactions.find(
                            (r) =>
                              r.messageId === msg.id &&
                              r.emoji === emoji &&
                              idStr(r.userId) === idStr(myIdentity!)
                          );
                          return (
                            <button
                              key={emoji}
                              className={`reaction-btn ${myReaction ? 'reaction-mine' : ''}`}
                              title={data.users.join(', ')}
                              onClick={() =>
                                conn?.reducers.reactToMessage({ messageId: msg.id, emoji })
                              }
                            >
                              {emoji} {data.count}
                            </button>
                          );
                        })}
                        <div className="reaction-add">
                          {['👍', '❤️', '😂', '😮', '😢'].map((emoji) => (
                            <button
                              key={emoji}
                              className="btn-react"
                              aria-label={`react ${emoji}`}
                              title={`React with ${emoji}`}
                              onClick={() =>
                                conn?.reducers.reactToMessage({ messageId: msg.id, emoji })
                              }
                            >
                              {emoji}
                            </button>
                          ))}
                        </div>
                      </div>

                      {/* Read receipts */}
                      {seenBy.length > 0 && (
                        <div className="read-receipt">Seen by {seenBy.join(', ')}</div>
                      )}
                    </div>
                  );
                })}
                <div ref={messagesEndRef} />
              </div>

              {/* Typing Indicator */}
              {typingUsers.length > 0 && (
                <div className="typing-indicator">
                  {typingUsers.length === 1
                    ? `${typingUsers[0]} is typing...`
                    : `${typingUsers.join(', ')} are typing...`}
                </div>
              )}

              {/* Message Input */}
              <div className="message-input-area">
                <div className="input-options">
                  <label className="ephemeral-label">
                    <select
                      value={isEphemeral ? String(ephemeralDuration) : 'off'}
                      onChange={(e) => {
                        if (e.target.value === 'off') {
                          setIsEphemeral(false);
                        } else {
                          setIsEphemeral(true);
                          setEphemeralDuration(Number(e.target.value));
                        }
                      }}
                      aria-label="Ephemeral duration"
                    >
                      <option value="off">Normal</option>
                      <option value="30">Ephemeral 30s</option>
                      <option value="60">Ephemeral 1m</option>
                      <option value="300">Ephemeral 5m</option>
                      <option value="3600">Ephemeral 1h</option>
                    </select>
                  </label>
                  <button
                    className={`btn-sm ${showSchedule ? 'btn-active' : ''}`}
                    onClick={() => setShowSchedule(!showSchedule)}
                    title="Schedule message"
                    aria-label="Schedule message"
                  >
                    Schedule
                  </button>
                </div>

                {showSchedule && (
                  <div className="schedule-picker">
                    <input
                      type="datetime-local"
                      value={scheduleTime}
                      onChange={(e) => setScheduleTime(e.target.value)}
                    />
                  </div>
                )}

                <div className="input-row">
                  <input
                    type="text"
                    className="message-input"
                    placeholder="Type a message..."
                    value={messageInput}
                    onChange={(e) => {
                      setMessageInput(e.target.value);
                      sendTyping();
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        handleSendMessage();
                      }
                    }}
                  />
                  <button className="btn-send" onClick={handleSendMessage}>
                    Send
                  </button>
                </div>
              </div>
            </>
          ) : (
            <div className="no-room-selected">
              <p>Select a room or create a new one to start chatting.</p>
            </div>
          )}
        </main>

        {/* Right Sidebar: Members */}
        {showMembers && selectedRoom && (
          <aside className="members-sidebar">
            <div className="sidebar-header">Members</div>
            <ul className="member-list">
              {currentRoomMembers.map((rm) => {
                const u = users.find((u) => idStr(u.identity) === idStr(rm.userId));
                if (!u) return null;
                const isOnline = u.online && u.status !== 'invisible';
                const lastActiveDate = tsToDate(u.lastActive);
                const isTargetMe = idStr(rm.userId) === idStr(myIdentity!);
                return (
                  <li key={String(rm.id)} className="member-item">
                    <span
                      className="status-dot"
                      style={{ background: statusColor(u.status, u.online) }}
                      title={u.status}
                    />
                    <span className="member-name">{u.name}</span>
                    {rm.isAdmin && <span className="admin-badge">Admin</span>}
                    {!isOnline && (
                      <span className="last-active" title={lastActiveDate.toLocaleString()}>
                        Last active {timeAgo(lastActiveDate)}
                      </span>
                    )}
                    {myMembership?.isAdmin && !isTargetMe && !rm.isAdmin && (
                      <div className="member-actions">
                        <button
                          className="btn-sm btn-danger"
                          onClick={() =>
                            conn?.reducers.kickUser({
                              roomId: selectedRoomId!,
                              userId: rm.userId,
                            })
                          }
                        >
                          Kick
                        </button>
                        <button
                          className="btn-sm"
                          onClick={() =>
                            conn?.reducers.promoteUser({
                              roomId: selectedRoomId!,
                              userId: rm.userId,
                            })
                          }
                        >
                          Promote
                        </button>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          </aside>
        )}
      </div>

      {/* Edit History Modal */}
      {historyMessageId !== null && (
        <div className="modal-overlay" onClick={() => setHistoryMessageId(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              Edit History
              <button className="btn-close" onClick={() => setHistoryMessageId(null)}>
                ×
              </button>
            </div>
            <div className="modal-body">
              {historyEntries.length === 0 ? (
                <p>No history available.</p>
              ) : (
                historyEntries.map((h) => (
                  <div key={String(h.id)} className="history-entry">
                    <span className="history-text">{h.text}</span>
                    <span className="history-time">
                      {tsToDate(h.editedAt).toLocaleString()}
                    </span>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
