import { useState, useEffect, useRef, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message } from './module_bindings/types';

// ─────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────
function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function timeAgo(date: Date): string {
  const secs = Math.floor((Date.now() - date.getTime()) / 1000);
  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ago`;
  return `${Math.floor(secs / 86400)}d ago`;
}

const STATUS_EMOJI: Record<string, string> = {
  online: '🟢',
  away: '🟡',
  dnd: '🔴',
  invisible: '⚫',
};

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

// ─────────────────────────────────────────
// Main App
// ─────────────────────────────────────────
export default function App() {
  const { isActive, identity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;
  const [subscribed, setSubscribed] = useState(false);

  // Save token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe
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
        'SELECT * FROM reaction',
        'SELECT * FROM read_receipt',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [messageEdits] = useTable(tables.messageEdit);
  const [reactions] = useTable(tables.reaction);
  const [readReceipts] = useTable(tables.readReceipt);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // UI state
  const [username, setUsername] = useState('');
  const [registered, setRegistered] = useState(false);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showNewRoom, setShowNewRoom] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [isScheduled, setIsScheduled] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [showHistoryFor, setShowHistoryFor] = useState<bigint | null>(null);
  const [showReactionsFor, setShowReactionsFor] = useState<bigint | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inactivityTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Scroll to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // Check if registered (my identity is in users table)
  useEffect(() => {
    if (!identity || !subscribed) return;
    const me = users.find(u => u.identity.toHexString() === identity.toHexString());
    if (me) setRegistered(true);
  }, [users, identity, subscribed]);

  // Inactivity => away after 5 min
  const resetInactivity = useCallback(() => {
    if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
    inactivityTimerRef.current = setTimeout(() => {
      conn?.reducers.setStatus({ status: 'away' });
    }, 5 * 60 * 1000);
    conn?.reducers.updateActivity({});
  }, [conn]);

  useEffect(() => {
    const handler = () => resetInactivity();
    window.addEventListener('mousemove', handler);
    window.addEventListener('keydown', handler);
    return () => {
      window.removeEventListener('mousemove', handler);
      window.removeEventListener('keydown', handler);
    };
  }, [resetInactivity]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!selectedRoomId || !identity || !conn || !subscribed) return;
    const roomMessages = messages
      .filter(m => m.roomId === selectedRoomId && !m.isDeleted)
      .sort((a, b) => (a.id < b.id ? -1 : 1));
    if (roomMessages.length === 0) return;
    const lastMsg = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [selectedRoomId, messages, identity, conn, subscribed]);

  const myHex = identity?.toHexString();

  const myMembership = (roomId: bigint) =>
    roomMembers.find(m => m.roomId === roomId && m.identity.toHexString() === myHex && !m.isBanned);

  const isJoined = (roomId: bigint) => !!myMembership(roomId);

  const isAdmin = (roomId: bigint) => {
    const m = myMembership(roomId);
    return m?.isAdmin ?? false;
  };

  // Unread count per room
  const unreadCount = (roomId: bigint): number => {
    const myReceipt = readReceipts.find(
      r => r.roomId === roomId && r.identity.toHexString() === myHex
    );
    const lastRead = myReceipt?.lastReadMessageId ?? 0n;
    return messages.filter(
      m => m.roomId === roomId && !m.isDeleted && m.id > lastRead &&
        m.senderIdentity.toHexString() !== myHex
    ).length;
  };

  // Typing users in selected room (exclude self)
  const typingUsers = (roomId: bigint): string[] => {
    const now = Date.now();
    return typingIndicators
      .filter(ind => {
        if (ind.roomId !== roomId) return false;
        if (ind.identity.toHexString() === myHex) return false;
        // Expire after 5 seconds
        const expiresMs = Number(ind.expiresAt.microsSinceUnixEpoch / 1000n) + 5000;
        return now < expiresMs;
      })
      .map(ind => {
        const user = users.find(u => u.identity.toHexString() === ind.identity.toHexString());
        return user?.name ?? 'Someone';
      });
  };

  // Reactions for a message grouped by emoji
  const messageReactions = (messageId: bigint): Map<string, { count: number; userNames: string[]; myReaction: boolean }> => {
    const map = new Map<string, { count: number; userNames: string[]; myReaction: boolean }>();
    for (const r of reactions.filter(r => r.messageId === messageId)) {
      const existing = map.get(r.emoji) ?? { count: 0, userNames: [], myReaction: false };
      const userName = users.find(u => u.identity.toHexString() === r.userIdentity.toHexString())?.name ?? '?';
      existing.count++;
      existing.userNames.push(userName);
      if (r.userIdentity.toHexString() === myHex) existing.myReaction = true;
      map.set(r.emoji, existing);
    }
    return map;
  };

  // Seen by for a message
  const seenBy = (msg: Message): string[] => {
    return readReceipts
      .filter(r => r.roomId === msg.roomId && r.lastReadMessageId >= msg.id && r.identity.toHexString() !== myHex)
      .map(r => users.find(u => u.identity.toHexString() === r.identity.toHexString())?.name ?? '?');
  };

  // Room message list
  const roomMessages = selectedRoomId
    ? messages
        .filter(m => m.roomId === selectedRoomId)
        .sort((a, b) => (a.id < b.id ? -1 : 1))
    : [];

  // Handlers
  const handleRegister = () => {
    if (!username.trim() || !conn) return;
    conn.reducers.register({ name: username.trim() });
  };

  const handleSend = () => {
    if (!messageText.trim() || !conn || !selectedRoomId) return;
    if (isScheduled && scheduleTime) {
      const sendAtMs = new Date(scheduleTime).getTime();
      const sendAtMicros = BigInt(sendAtMs) * 1000n;
      conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageText.trim(), sendAtMicros });
    } else if (isEphemeral) {
      conn.reducers.sendEphemeralMessage({ roomId: selectedRoomId, text: messageText.trim(), durationSeconds: ephemeralDuration });
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    }
    setMessageText('');
    setIsEphemeral(false);
    setIsScheduled(false);
    // Clear typing
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    conn.reducers.clearTyping({ roomId: selectedRoomId });
  };

  const handleTyping = (text: string) => {
    setMessageText(text);
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.clearTyping({ roomId: selectedRoomId! });
    }, 4000);
  };

  const handleCreateRoom = () => {
    if (!newRoomName.trim() || !conn) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowNewRoom(false);
  };

  const handleEditSave = (messageId: bigint) => {
    if (!editText.trim() || !conn) return;
    conn.reducers.editMessage({ messageId, newText: editText.trim() });
    setEditingMessageId(null);
    setEditText('');
  };

  const handleKick = (roomId: bigint, targetIdentity: any) => {
    if (!conn) return;
    conn.reducers.kickUser({ roomId, targetIdentity });
  };

  const handlePromote = (roomId: bigint, targetIdentity: any) => {
    if (!conn) return;
    conn.reducers.promoteUser({ roomId, targetIdentity });
  };

  // Countdown for ephemeral messages
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  const ephemeralCountdown = (msg: Message): number => {
    if (!msg.isEphemeral) return 0;
    const sentMs = Number(msg.sentAt.microsSinceUnixEpoch / 1000n);
    const expiresMs = sentMs + msg.ephemeralDurationSeconds * 1000;
    return Math.max(0, Math.floor((expiresMs - now) / 1000));
  };

  // ─────────────────────────────────────────
  // Render: Registration
  // ─────────────────────────────────────────
  if (!subscribed) {
    return (
      <div className="loading-screen">
        <div className="loading-text">Connecting to SpacetimeDB Chat...</div>
      </div>
    );
  }

  if (!registered) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <h1 className="app-title gradient-text">SpacetimeDB Chat</h1>
          <p className="register-subtitle">Enter your display name to get started</p>
          <input
            className="text-input"
            type="text"
            placeholder="Your name..."
            value={username}
            onChange={e => setUsername(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            maxLength={30}
            autoFocus
          />
          <button className="btn-primary" onClick={handleRegister} disabled={!username.trim()}>
            Join Chat
          </button>
        </div>
      </div>
    );
  }

  const me = users.find(u => u.identity.toHexString() === myHex);
  const selectedRoom = rooms.find(r => r.id === selectedRoomId);

  // ─────────────────────────────────────────
  // Render: Main Chat
  // ─────────────────────────────────────────
  return (
    <div className="app-layout">
      {/* ── Sidebar ── */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <span className="app-title gradient-text">SpacetimeDB Chat</span>
        </div>

        {/* My status */}
        {me && (
          <div className="my-status">
            <span className="status-dot">{STATUS_EMOJI[me.status] ?? '🟢'}</span>
            <span className="my-name">{me.name}</span>
            <select
              className="status-select"
              value={me.status}
              onChange={e => conn?.reducers.setStatus({ status: e.target.value })}
            >
              <option value="online">Online</option>
              <option value="away">Away</option>
              <option value="dnd">Do Not Disturb</option>
              <option value="invisible">Invisible</option>
            </select>
          </div>
        )}

        {/* Rooms */}
        <div className="section-header">
          <span>Rooms</span>
          <button className="btn-icon" onClick={() => setShowNewRoom(v => !v)} title="New room">+</button>
        </div>

        {showNewRoom && (
          <div className="new-room-form">
            <input
              className="text-input small"
              placeholder="Room name..."
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
              autoFocus
              maxLength={50}
            />
            <button className="btn-primary small" onClick={handleCreateRoom}>Create</button>
          </div>
        )}

        <ul className="room-list">
          {rooms.map(room => {
            const joined = isJoined(room.id);
            const unread = joined ? unreadCount(room.id) : 0;
            return (
              <li
                key={String(room.id)}
                className={`room-item ${selectedRoomId === room.id ? 'active' : ''} ${!joined ? 'not-joined' : ''}`}
                onClick={() => {
                  setSelectedRoomId(room.id);
                  if (!joined) conn?.reducers.joinRoom({ roomId: room.id });
                }}
              >
                <span className="room-name"># {room.name}</span>
                {unread > 0 && <span className="unread-badge">{unread}</span>}
                {!joined && <span className="join-hint">click to join</span>}
              </li>
            );
          })}
        </ul>

        {/* Online users */}
        <div className="section-header">Users</div>
        <ul className="user-list">
          {users
            .filter(u => u.status !== 'invisible' || u.identity.toHexString() === myHex)
            .sort((a, b) => (a.online === b.online ? 0 : a.online ? -1 : 1))
            .map(u => (
              <li key={u.identity.toHexString()} className="user-item">
                <span className="status-dot">{STATUS_EMOJI[u.status] ?? '🟢'}</span>
                <span className={`user-name ${!u.online ? 'offline' : ''}`}>{u.name}</span>
                {!u.online && (
                  <span className="last-active">
                    {timeAgo(tsToDate(u.lastActive))}
                  </span>
                )}
              </li>
            ))}
        </ul>
      </aside>

      {/* ── Main Chat ── */}
      <main className="chat-main">
        {!selectedRoom ? (
          <div className="no-room">
            <p>Select or create a room to start chatting</p>
          </div>
        ) : (
          <>
            {/* Room header */}
            <header className="chat-header">
              <div className="chat-header-left">
                <span className="chat-room-name"># {selectedRoom.name}</span>
                {isAdmin(selectedRoom.id) && <span className="admin-badge">Admin</span>}
              </div>
              <div className="chat-header-right">
                <button
                  className="btn-secondary small"
                  onClick={() => {
                    conn?.reducers.leaveRoom({ roomId: selectedRoom.id });
                    setSelectedRoomId(null);
                  }}
                >
                  Leave
                </button>
              </div>
            </header>

            {/* Messages */}
            <div className="messages-container">
              {roomMessages.map(msg => {
                const sender = users.find(u => u.identity.toHexString() === msg.senderIdentity.toHexString());
                const isMe = msg.senderIdentity.toHexString() === myHex;
                const rxns = messageReactions(msg.id);
                const seen = seenBy(msg);
                const countdown = ephemeralCountdown(msg);
                const edits = messageEdits
                  .filter(e => e.messageId === msg.id)
                  .sort((a, b) => (a.id < b.id ? -1 : 1));
                const isEditing = editingMessageId === msg.id;

                if (msg.isDeleted) {
                  return (
                    <div key={String(msg.id)} className={`message-row ${isMe ? 'mine' : ''}`}>
                      <div className="message-bubble deleted">[deleted]</div>
                    </div>
                  );
                }

                return (
                  <div key={String(msg.id)} className={`message-row ${isMe ? 'mine' : ''}`}>
                    <div className={`message-bubble ${msg.isEphemeral ? 'ephemeral' : ''}`}>
                      <div className="message-header">
                        <span className="message-sender">{sender?.name ?? '?'}</span>
                        <span className="message-time">{formatTime(tsToDate(msg.sentAt))}</span>
                        {msg.isEphemeral && countdown > 0 && (
                          <span className="ephemeral-countdown">⏱ {countdown}s</span>
                        )}
                        {msg.editedAt && <span className="edited-indicator">(edited)</span>}
                        {isMe && !msg.isEphemeral && (
                          <button
                            className="btn-icon tiny"
                            onClick={() => { setEditingMessageId(msg.id); setEditText(msg.text); }}
                          >
                            ✏️
                          </button>
                        )}
                        {edits.length > 0 && (
                          <button
                            className="btn-icon tiny"
                            onClick={() => setShowHistoryFor(showHistoryFor === msg.id ? null : msg.id)}
                          >
                            📜
                          </button>
                        )}
                      </div>

                      {isEditing ? (
                        <div className="edit-form">
                          <input
                            className="text-input small"
                            value={editText}
                            onChange={e => setEditText(e.target.value)}
                            onKeyDown={e => { if (e.key === 'Enter') handleEditSave(msg.id); if (e.key === 'Escape') setEditingMessageId(null); }}
                            autoFocus
                          />
                          <button className="btn-primary small" onClick={() => handleEditSave(msg.id)}>Save</button>
                          <button className="btn-secondary small" onClick={() => setEditingMessageId(null)}>Cancel</button>
                        </div>
                      ) : (
                        <div className="message-text">{msg.text}</div>
                      )}

                      {/* Edit history */}
                      {showHistoryFor === msg.id && edits.length > 0 && (
                        <div className="edit-history">
                          <div className="history-title">Edit History</div>
                          {edits.map(e => (
                            <div key={String(e.id)} className="history-entry">
                              <span className="history-text">{e.previousText}</span>
                              <span className="history-time">{formatTime(tsToDate(e.editedAt))}</span>
                            </div>
                          ))}
                        </div>
                      )}

                      {/* Reactions */}
                      <div className="reactions-row">
                        {[...rxns.entries()].map(([emoji, data]) => (
                          <button
                            key={emoji}
                            className={`reaction-btn ${data.myReaction ? 'mine' : ''}`}
                            title={data.userNames.join(', ')}
                            onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                          >
                            {emoji} {data.count}
                          </button>
                        ))}
                        <button
                          className="reaction-add-btn"
                          onClick={() => setShowReactionsFor(showReactionsFor === msg.id ? null : msg.id)}
                        >
                          +😀
                        </button>
                        {showReactionsFor === msg.id && (
                          <div className="emoji-picker">
                            {REACTION_EMOJIS.map(e => (
                              <button
                                key={e}
                                className="emoji-opt"
                                onClick={() => {
                                  conn?.reducers.toggleReaction({ messageId: msg.id, emoji: e });
                                  setShowReactionsFor(null);
                                }}
                              >
                                {e}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>

                      {/* Seen by */}
                      {isMe && seen.length > 0 && (
                        <div className="seen-by">Seen by {seen.join(', ')}</div>
                      )}
                    </div>

                    {/* Admin: kick from room context */}
                    {isAdmin(selectedRoom.id) && !isMe && (
                      <button
                        className="btn-icon tiny kick-btn"
                        title="Kick user"
                        onClick={() => handleKick(selectedRoom.id, msg.senderIdentity)}
                      >
                        🚫
                      </button>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {(() => {
              const typing = typingUsers(selectedRoom.id);
              if (typing.length === 0) return null;
              const label = typing.length === 1
                ? `${typing[0]} is typing...`
                : `${typing.join(', ')} are typing...`;
              return <div className="typing-indicator">{label}</div>;
            })()}

            {/* Scheduled messages panel */}
            {(() => {
              const myScheduled = scheduledMessages.filter(
                s => s.roomId === selectedRoom.id && s.authorIdentity.toHexString() === myHex && !s.cancelled
              );
              if (myScheduled.length === 0) return null;
              return (
                <div className="scheduled-panel">
                  <div className="scheduled-title">Scheduled Messages</div>
                  {myScheduled.map(s => (
                    <div key={String(s.id)} className="scheduled-item">
                      <span className="scheduled-text">{s.text}</span>
                      <span className="scheduled-time">at {formatTime(tsToDate(s.sendAt))}</span>
                      <button
                        className="btn-icon tiny"
                        onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledMessageId: s.id })}
                      >
                        ✕
                      </button>
                    </div>
                  ))}
                </div>
              );
            })()}

            {/* Input area */}
            <div className="input-area">
              <div className="input-options">
                <label className="option-toggle">
                  <input
                    type="checkbox"
                    checked={isEphemeral}
                    onChange={e => { setIsEphemeral(e.target.checked); if (e.target.checked) setIsScheduled(false); }}
                  />
                  <span>Ephemeral</span>
                </label>
                {isEphemeral && (
                  <select
                    className="duration-select"
                    value={ephemeralDuration}
                    onChange={e => setEphemeralDuration(Number(e.target.value))}
                  >
                    <option value={60}>1 min</option>
                    <option value={300}>5 min</option>
                    <option value={3600}>1 hr</option>
                  </select>
                )}
                <label className="option-toggle">
                  <input
                    type="checkbox"
                    checked={isScheduled}
                    onChange={e => { setIsScheduled(e.target.checked); if (e.target.checked) setIsEphemeral(false); }}
                  />
                  <span>Schedule</span>
                </label>
                {isScheduled && (
                  <input
                    type="datetime-local"
                    className="datetime-input"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                  />
                )}
              </div>

              <div className="message-input-row">
                <input
                  className="message-input"
                  type="text"
                  placeholder={`Message #${selectedRoom.name}...`}
                  value={messageText}
                  onChange={e => handleTyping(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && !e.shiftKey && handleSend()}
                  maxLength={2000}
                />
                <button
                  className="btn-primary send-btn"
                  onClick={handleSend}
                  disabled={!messageText.trim() || (isScheduled && !scheduleTime)}
                >
                  Send
                </button>
              </div>
            </div>

            {/* Room members / admin panel */}
            {isAdmin(selectedRoom.id) && (
              <div className="members-panel">
                <div className="members-title">Members</div>
                {roomMembers
                  .filter(m => m.roomId === selectedRoom.id)
                  .map(m => {
                    const memberUser = users.find(u => u.identity.toHexString() === m.identity.toHexString());
                    if (m.identity.toHexString() === myHex) return null;
                    return (
                      <div key={m.identity.toHexString()} className={`member-item ${m.isBanned ? 'banned' : ''}`}>
                        <span>{memberUser?.name ?? '?'}</span>
                        {m.isAdmin && <span className="admin-badge">Admin</span>}
                        {m.isBanned && <span className="banned-badge">Banned</span>}
                        {!m.isBanned && (
                          <>
                            <button className="btn-icon tiny" onClick={() => handleKick(selectedRoom.id, m.identity)}>🚫 Kick</button>
                            {!m.isAdmin && (
                              <button className="btn-icon tiny" onClick={() => handlePromote(selectedRoom.id, m.identity)}>⬆️ Promote</button>
                            )}
                          </>
                        )}
                      </div>
                    );
                  })}
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}
