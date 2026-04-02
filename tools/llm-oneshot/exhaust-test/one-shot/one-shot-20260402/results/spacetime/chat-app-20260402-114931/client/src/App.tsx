import { useState, useEffect, useRef, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { User, RoomMember, MessageEdit, ScheduledMessage } from './module_bindings/types';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function timeAgo(date: Date): string {
  const secs = Math.floor((Date.now() - date.getTime()) / 1000);
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

function countdown(expiresAt: { microsSinceUnixEpoch: bigint }): string {
  const remaining = Number(expiresAt.microsSinceUnixEpoch / 1000n) - Date.now();
  if (remaining <= 0) return 'expired';
  const secs = Math.ceil(remaining / 1000);
  if (secs < 60) return `${secs}s`;
  return `${Math.ceil(secs / 60)}m`;
}

const EMOJI_LIST = ['👍', '❤️', '😂', '😮', '😢'];
const STATUS_OPTIONS = ['online', 'away', 'dnd', 'invisible'] as const;
type UserStatus = typeof STATUS_OPTIONS[number];

const STATUS_LABELS: Record<UserStatus, string> = {
  online: '🟢 Online',
  away: '🟡 Away',
  dnd: '🔴 Do Not Disturb',
  invisible: '⚫ Invisible',
};

// ─── Main Component ───────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe to all tables
  const [subscribed, setSubscribed] = useState(false);
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM message_edit',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM user_room_read',
        'SELECT * FROM reaction',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  // Table data — useTable returns [ReadonlyArray<RowType>, boolean]
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [messageEdits] = useTable(tables.messageEdit);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [userRoomReads] = useTable(tables.userRoomRead);
  const [reactions] = useTable(tables.reaction);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // UI state
  const [registerName, setRegisterName] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduledTime, setScheduledTime] = useState('');
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [showHistoryFor, setShowHistoryFor] = useState<bigint | null>(null);
  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const [, setTick] = useState(0);

  // Refresh countdown timers every second
  useEffect(() => {
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Current user
  const myUser = myIdentity
    ? users.find(u => u.identity.toHexString() === myIdentity.toHexString())
    : null;

  const isRegistered = !!myUser;

  // Auto-scroll messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, selectedRoomId]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!conn || !selectedRoomId || !isRegistered) return;
    const roomMessages = messages
      .filter(m => m.roomId === selectedRoomId && !m.isDeleted)
      .sort((a, b) => (a.id < b.id ? -1 : 1));
    if (roomMessages.length === 0) return;
    const lastMsg = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [conn, selectedRoomId, messages.length, isRegistered]);

  // ── Registration ─────────────────────────────────────────────────────────────

  const handleRegister = useCallback(() => {
    if (!conn || !registerName.trim()) return;
    conn.reducers.register({ name: registerName.trim() });
    setRegisterName('');
  }, [conn, registerName]);

  // ── Typing indicators ────────────────────────────────────────────────────────

  const handleMessageInput = useCallback((value: string) => {
    setMessageText(value);
    if (!conn || !selectedRoomId) return;
    conn.reducers.updateTyping({ roomId: selectedRoomId, isTyping: true });
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      conn.reducers.updateTyping({ roomId: selectedRoomId, isTyping: false });
    }, 4000);
  }, [conn, selectedRoomId]);

  // ── Send message ─────────────────────────────────────────────────────────────

  const handleSend = useCallback(() => {
    if (!conn || !selectedRoomId || !messageText.trim()) return;
    if (isEphemeral) {
      conn.reducers.sendEphemeralMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        durationSeconds: ephemeralDuration,
      });
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    }
    setMessageText('');
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    if (conn && selectedRoomId) {
      conn.reducers.updateTyping({ roomId: selectedRoomId, isTyping: false });
    }
  }, [conn, selectedRoomId, messageText, isEphemeral, ephemeralDuration]);

  // ── Schedule message ─────────────────────────────────────────────────────────

  const handleScheduleSubmit = useCallback(() => {
    if (!conn || !selectedRoomId || !messageText.trim() || !scheduledTime) return;
    const dateMs = new Date(scheduledTime).getTime();
    const sendAtMicros = BigInt(dateMs) * 1000n;
    conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageText.trim(), sendAtMicros });
    setShowScheduleModal(false);
    setMessageText('');
    setScheduledTime('');
  }, [conn, selectedRoomId, messageText, scheduledTime]);

  // ── Derived data ─────────────────────────────────────────────────────────────

  const roomMessages = selectedRoomId !== null
    ? messages
        .filter(m => m.roomId === selectedRoomId && !m.isDeleted)
        .sort((a, b) => (a.sentAt.microsSinceUnixEpoch < b.sentAt.microsSinceUnixEpoch ? -1 : 1))
    : [];

  const typingInRoom = selectedRoomId !== null
    ? typingIndicators.filter(t => {
        if (t.roomId !== selectedRoomId) return false;
        if (!myIdentity) return false;
        if (t.userId.toHexString() === myIdentity.toHexString()) return false;
        // Filter expired (cleanup timer handles deletion, but be safe client-side)
        const expires = Number(t.expiresAt.microsSinceUnixEpoch / 1000n);
        return expires > Date.now();
      })
    : [];

  const typingNames = typingInRoom
    .map(t => users.find(u => u.identity.toHexString() === t.userId.toHexString())?.name ?? 'Someone')
    .filter(Boolean);

  function getUnreadCount(roomId: bigint): number {
    const roomMsgs = messages.filter(m => m.roomId === roomId && !m.isDeleted);
    if (roomMsgs.length === 0) return 0;
    if (!myIdentity) return 0;
    const readState = userRoomReads.find(
      r => r.userId.toHexString() === myIdentity.toHexString() && r.roomId === roomId
    );
    if (!readState) return roomMsgs.length;
    return roomMsgs.filter(m => m.id > readState.lastReadMessageId).length;
  }

  function getSeenBy(messageId: bigint): string[] {
    return readReceipts
      .filter(r => r.messageId === messageId)
      .map(r => {
        const u = users.find(u => u.identity.toHexString() === r.userId.toHexString());
        return u?.name ?? '?';
      });
  }

  function getReactionsForMessage(messageId: bigint): Map<string, { count: number; users: string[]; myReaction: boolean }> {
    const map = new Map<string, { count: number; users: string[]; myReaction: boolean }>();
    reactions
      .filter(r => r.messageId === messageId)
      .forEach(r => {
        const existing = map.get(r.emoji) ?? { count: 0, users: [], myReaction: false };
        existing.count++;
        const u = users.find(u => u.identity.toHexString() === r.userId.toHexString());
        existing.users.push(u?.name ?? '?');
        if (myIdentity && r.userId.toHexString() === myIdentity.toHexString()) {
          existing.myReaction = true;
        }
        map.set(r.emoji, existing);
      });
    return map;
  }

  function getEditHistory(messageId: bigint): MessageEdit[] {
    return messageEdits
      .filter(e => e.messageId === messageId)
      .sort((a, b) => (a.editedAt.microsSinceUnixEpoch < b.editedAt.microsSinceUnixEpoch ? -1 : 1));
  }

  function isAdminInRoom(roomId: bigint): boolean {
    if (!myIdentity) return false;
    const member = roomMembers.find(
      m => m.roomId === roomId && m.identity.toHexString() === myIdentity.toHexString()
    );
    return member?.isAdmin ?? false;
  }

  function getRoomMembers(roomId: bigint): { member: RoomMember; user: User | undefined }[] {
    return roomMembers
      .filter(m => m.roomId === roomId && !m.isBanned)
      .map(m => ({
        member: m,
        user: users.find(u => u.identity.toHexString() === m.identity.toHexString()),
      }));
  }

  function getMyScheduledMessages(): ScheduledMessage[] {
    if (!myIdentity) return [];
    return scheduledMessages.filter(
      s => s.sender.toHexString() === myIdentity.toHexString() && s.roomId === selectedRoomId
    );
  }

  // ── Not connected/loading ─────────────────────────────────────────────────────

  if (!isActive) {
    return (
      <div className="loading">
        <div className="loading-text">Connecting to SpacetimeDB Chat...</div>
      </div>
    );
  }

  if (!subscribed) {
    return (
      <div className="loading">
        <div className="loading-text">Loading data...</div>
      </div>
    );
  }

  // ── Registration screen ───────────────────────────────────────────────────────

  if (!isRegistered) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <h1>SpacetimeDB Chat</h1>
          <p>Choose your display name to get started.</p>
          <div className="register-form">
            <input
              type="text"
              placeholder="Display name..."
              value={registerName}
              onChange={e => setRegisterName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleRegister()}
              maxLength={32}
              autoFocus
            />
            <button onClick={handleRegister} disabled={!registerName.trim()}>
              Join Chat
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── Main Chat UI ──────────────────────────────────────────────────────────────

  const myScheduledMessages = getMyScheduledMessages();
  const roomMemberList = selectedRoomId ? getRoomMembers(selectedRoomId) : [];
  const amAdmin = selectedRoomId ? isAdminInRoom(selectedRoomId) : false;

  return (
    <div className="app">
      {/* ── Sidebar ── */}
      <aside className="sidebar">
        {/* User Info + Status */}
        <div className="sidebar-header">
          <div className="user-info">
            <span className="user-name">{myUser?.name}</span>
            <select
              className="status-select"
              value={myUser?.status ?? 'online'}
              onChange={e => conn?.reducers.setStatus({ status: e.target.value })}
            >
              {STATUS_OPTIONS.map(s => (
                <option key={s} value={s}>{STATUS_LABELS[s]}</option>
              ))}
            </select>
          </div>
          <h2>SpacetimeDB Chat</h2>
        </div>

        {/* Room List */}
        <div className="rooms-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowCreateRoom(!showCreateRoom)} title="Create room">+</button>
          </div>
          {showCreateRoom && (
            <div className="create-room-form">
              <input
                type="text"
                placeholder="Room name..."
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter' && newRoomName.trim()) {
                    conn?.reducers.createRoom({ name: newRoomName.trim() });
                    setNewRoomName('');
                    setShowCreateRoom(false);
                  }
                }}
                autoFocus
              />
              <button onClick={() => {
                if (newRoomName.trim()) {
                  conn?.reducers.createRoom({ name: newRoomName.trim() });
                  setNewRoomName('');
                  setShowCreateRoom(false);
                }
              }}>Create</button>
            </div>
          )}
          <div className="room-list">
            {rooms.map(room => {
              const isMember = myIdentity && roomMembers.some(
                m => m.roomId === room.id && m.identity.toHexString() === myIdentity.toHexString() && !m.isBanned
              );
              const unread = isMember ? getUnreadCount(room.id) : 0;
              return (
                <div
                  key={room.id.toString()}
                  className={`room-item ${selectedRoomId === room.id ? 'active' : ''} ${!isMember ? 'not-member' : ''}`}
                  onClick={() => {
                    if (!isMember) {
                      conn?.reducers.joinRoom({ roomId: room.id });
                    }
                    setSelectedRoomId(room.id);
                    setShowAdminPanel(false);
                  }}
                >
                  <span className="room-name"># {room.name}</span>
                  {!isMember && <span className="join-hint">click to join</span>}
                  {isMember && unread > 0 && (
                    <span className="unread-badge">{unread}</span>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Online Users */}
        <div className="users-section">
          <div className="section-header"><span>Users</span></div>
          <div className="user-list">
            {users
              .filter(u => u.status !== 'invisible' || (myIdentity && u.identity.toHexString() === myIdentity.toHexString()))
              .sort((a, b) => {
                if (a.online && !b.online) return -1;
                if (!a.online && b.online) return 1;
                return a.name.localeCompare(b.name);
              })
              .map(u => {
                const isMe = myIdentity && u.identity.toHexString() === myIdentity.toHexString();
                const lastActive = u.online ? null : tsToDate(u.lastActive);
                return (
                  <div key={u.identity.toHexString()} className={`user-item ${u.status}`}>
                    <span className={`status-dot ${u.status}`} title={u.status} />
                    <span className="user-name">{u.name}{isMe ? ' (you)' : ''}</span>
                    {!u.online && lastActive && (
                      <span className="last-active" title={lastActive.toLocaleString()}>
                        {timeAgo(lastActive)}
                      </span>
                    )}
                  </div>
                );
              })}
          </div>
        </div>
      </aside>

      {/* ── Main content ── */}
      <main className="main">
        {selectedRoomId === null ? (
          <div className="no-room">
            <p>Select or join a room to start chatting.</p>
          </div>
        ) : (
          <>
            {/* Room header */}
            <div className="room-header">
              <h2># {rooms.find(r => r.id === selectedRoomId)?.name}</h2>
              <div className="room-actions">
                {amAdmin && (
                  <button
                    className={`btn-secondary ${showAdminPanel ? 'active' : ''}`}
                    onClick={() => setShowAdminPanel(!showAdminPanel)}
                  >
                    Admin
                  </button>
                )}
                <button
                  className="btn-secondary"
                  onClick={() => {
                    conn?.reducers.leaveRoom({ roomId: selectedRoomId });
                    setSelectedRoomId(null);
                  }}
                >
                  Leave
                </button>
              </div>
            </div>

            {/* Admin panel */}
            {showAdminPanel && amAdmin && (
              <div className="admin-panel">
                <h3>Room Members</h3>
                {roomMemberList.map(({ member, user }) => {
                  const isMe = myIdentity && member.identity.toHexString() === myIdentity.toHexString();
                  return (
                    <div key={member.id.toString()} className="admin-member-row">
                      <span>{user?.name ?? 'Unknown'} {member.isAdmin ? '(admin)' : ''} {isMe ? '(you)' : ''}</span>
                      {!isMe && (
                        <div className="admin-actions">
                          <button
                            className="btn-danger btn-sm"
                            onClick={() => conn?.reducers.kickUser({ roomId: selectedRoomId, targetIdentity: member.identity })}
                          >
                            Kick
                          </button>
                          <button
                            className="btn-danger btn-sm"
                            onClick={() => conn?.reducers.banUser({ roomId: selectedRoomId, targetIdentity: member.identity })}
                          >
                            Ban
                          </button>
                          {!member.isAdmin && (
                            <button
                              className="btn-secondary btn-sm"
                              onClick={() => conn?.reducers.promoteToAdmin({ roomId: selectedRoomId, targetIdentity: member.identity })}
                            >
                              Promote
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {/* Scheduled messages for this room */}
            {myScheduledMessages.length > 0 && (
              <div className="scheduled-panel">
                <h4>⏰ Your Scheduled Messages</h4>
                {myScheduledMessages.map(sm => {
                  const sa = sm.scheduledAt as { tag: string; value: { microsSinceUnixEpoch: bigint } | undefined };
                  const when = sa.tag === 'Time' && sa.value
                    ? tsToDate(sa.value).toLocaleString()
                    : 'pending';
                  return (
                    <div key={sm.scheduledId.toString()} className="scheduled-item">
                      <span className="scheduled-text">{sm.text}</span>
                      <span className="scheduled-time">{when}</span>
                      <button
                        className="btn-danger btn-sm"
                        onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: sm.scheduledId })}
                      >
                        Cancel
                      </button>
                    </div>
                  );
                })}
              </div>
            )}

            {/* Messages */}
            <div className="messages">
              {roomMessages.map(msg => {
                const sender = users.find(u => u.identity.toHexString() === msg.sender.toHexString());
                const isMe = myIdentity && msg.sender.toHexString() === myIdentity.toHexString();
                const seenBy = getSeenBy(msg.id).filter(
                  name => !myIdentity || name !== myUser?.name
                );
                const msgReactions = getReactionsForMessage(msg.id);
                const history = getEditHistory(msg.id);
                const isEditing = editingMessageId === msg.id;

                return (
                  <div key={msg.id.toString()} className={`message ${isMe ? 'mine' : ''}`}>
                    <div className="message-header">
                      <span className="message-sender">{sender?.name ?? 'Unknown'}</span>
                      <span className="message-time" title={tsToDate(msg.sentAt).toLocaleString()}>
                        {tsToDate(msg.sentAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                      </span>
                      {msg.isEphemeral && msg.expiresAt && (
                        <span className="ephemeral-badge" title="Disappears in...">
                          ⏳ {countdown(msg.expiresAt)}
                        </span>
                      )}
                      {msg.editedAt && <span className="edited-badge">(edited)</span>}
                    </div>

                    {isEditing ? (
                      <div className="edit-form">
                        <input
                          type="text"
                          value={editText}
                          onChange={e => setEditText(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Enter' && editText.trim()) {
                              conn?.reducers.editMessage({ messageId: msg.id, newText: editText.trim() });
                              setEditingMessageId(null);
                            }
                            if (e.key === 'Escape') setEditingMessageId(null);
                          }}
                          autoFocus
                        />
                        <button onClick={() => {
                          if (editText.trim()) {
                            conn?.reducers.editMessage({ messageId: msg.id, newText: editText.trim() });
                          }
                          setEditingMessageId(null);
                        }}>Save</button>
                        <button onClick={() => setEditingMessageId(null)}>Cancel</button>
                      </div>
                    ) : (
                      <div className="message-body">
                        <span className="message-text">{msg.text}</span>
                        {isMe && (
                          <div className="message-actions">
                            <button
                              className="msg-action-btn"
                              title="Edit"
                              onClick={() => {
                                setEditingMessageId(msg.id);
                                setEditText(msg.text);
                              }}
                            >✏️</button>
                            {history.length > 0 && (
                              <button
                                className="msg-action-btn"
                                title="Edit history"
                                onClick={() => setShowHistoryFor(msg.id)}
                              >📋</button>
                            )}
                          </div>
                        )}
                        {!isMe && history.length > 0 && (
                          <button
                            className="msg-action-btn"
                            title="View edit history"
                            onClick={() => setShowHistoryFor(msg.id)}
                          >📋</button>
                        )}
                      </div>
                    )}

                    {/* Reactions */}
                    <div className="reactions-row">
                      {[...msgReactions.entries()].map(([emoji, data]) => (
                        <button
                          key={emoji}
                          className={`reaction-btn ${data.myReaction ? 'my-reaction' : ''}`}
                          title={data.users.join(', ')}
                          onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                        >
                          {emoji} {data.count}
                        </button>
                      ))}
                      <div className="emoji-picker">
                        {EMOJI_LIST.map(emoji => (
                          <button
                            key={emoji}
                            className="add-reaction-btn"
                            onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Seen by */}
                    {seenBy.length > 0 && (
                      <div className="seen-by">Seen by {seenBy.join(', ')}</div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {typingNames.length > 0 && (
              <div className="typing-indicator">
                {typingNames.length === 1
                  ? `${typingNames[0]} is typing...`
                  : `${typingNames.slice(0, -1).join(', ')} and ${typingNames[typingNames.length - 1]} are typing...`}
              </div>
            )}

            {/* Message input */}
            <div className="message-input-area">
              <div className="input-options">
                <label className="ephemeral-toggle">
                  <input
                    type="checkbox"
                    checked={isEphemeral}
                    onChange={e => setIsEphemeral(e.target.checked)}
                  />
                  Ephemeral
                </label>
                {isEphemeral && (
                  <select
                    value={ephemeralDuration}
                    onChange={e => setEphemeralDuration(Number(e.target.value))}
                    className="duration-select"
                  >
                    <option value={30}>30s</option>
                    <option value={60}>1m</option>
                    <option value={300}>5m</option>
                    <option value={600}>10m</option>
                  </select>
                )}
              </div>
              <div className="input-row">
                <input
                  type="text"
                  placeholder={isEphemeral ? `Ephemeral message (${ephemeralDuration}s)...` : 'Type a message...'}
                  value={messageText}
                  onChange={e => handleMessageInput(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && !e.shiftKey && handleSend()}
                  className="message-input"
                />
                <button onClick={handleSend} disabled={!messageText.trim()} className="send-btn">
                  Send
                </button>
                <button
                  onClick={() => setShowScheduleModal(true)}
                  className="schedule-btn"
                  title="Schedule message"
                  disabled={!messageText.trim()}
                >
                  ⏰
                </button>
              </div>
            </div>
          </>
        )}
      </main>

      {/* ── Schedule Modal ── */}
      {showScheduleModal && (
        <div className="modal-overlay" onClick={() => setShowScheduleModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Schedule Message</h3>
            <p className="modal-preview">&ldquo;{messageText}&rdquo;</p>
            <div className="modal-form">
              <label>Send at:</label>
              <input
                type="datetime-local"
                value={scheduledTime}
                min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}
                onChange={e => setScheduledTime(e.target.value)}
              />
            </div>
            <div className="modal-actions">
              <button
                onClick={handleScheduleSubmit}
                disabled={!scheduledTime}
                className="btn-primary"
              >
                Schedule
              </button>
              <button onClick={() => setShowScheduleModal(false)} className="btn-secondary">
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Edit History Modal ── */}
      {showHistoryFor !== null && (
        <div className="modal-overlay" onClick={() => setShowHistoryFor(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            {(() => {
              const history = getEditHistory(showHistoryFor);
              const currentMsg = messages.find(m => m.id === showHistoryFor);
              return (
                <div className="edit-history">
                  {history.map((edit, i) => (
                    <div key={edit.id.toString()} className="history-entry">
                      <span className="history-version">v{i + 1}</span>
                      <span className="history-text">{edit.text}</span>
                      <span className="history-time">{tsToDate(edit.editedAt).toLocaleString()}</span>
                    </div>
                  ))}
                  {currentMsg && (
                    <div className="history-entry current">
                      <span className="history-version">current</span>
                      <span className="history-text">{currentMsg.text}</span>
                    </div>
                  )}
                </div>
              );
            })()}
            <button onClick={() => setShowHistoryFor(null)} className="btn-secondary">Close</button>
          </div>
        </div>
      )}
    </div>
  );
}

