import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, Room, RoomMember, User, MessageReaction, MessageRead, RoomReadPosition, MessageEditHistory, ScheduledMessage, TypingIndicator } from './module_bindings/types';

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '🔥'];
const EPHEMERAL_OPTIONS = [
  { label: '30s', secs: 30n },
  { label: '1m', secs: 60n },
  { label: '5m', secs: 300n },
  { label: '1h', secs: 3600n },
];

function statusIcon(status: string, online: boolean): string {
  if (!online) return '⚫';
  switch (status) {
    case 'away': return '🟡';
    case 'dnd': return '🔴';
    case 'invisible': return '⚫';
    default: return '🟢';
  }
}

function formatTime(micros: bigint): string {
  return new Date(Number(micros / 1000n)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatDate(micros: bigint): string {
  return new Date(Number(micros / 1000n)).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function timeAgo(micros: bigint): string {
  const diffSecs = Math.floor(Number((BigInt(Date.now()) * 1000n - micros) / 1_000_000n));
  if (diffSecs < 60) return 'just now';
  if (diffSecs < 3600) return `${Math.floor(diffSecs / 60)}m ago`;
  if (diffSecs < 86400) return `${Math.floor(diffSecs / 3600)}h ago`;
  return `${Math.floor(diffSecs / 86400)}d ago`;
}

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;
  const [subscribed, setSubscribed] = useState(false);

  // Tables
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [messageReactions] = useTable(tables.messageReaction);
  const [messageReads] = useTable(tables.messageRead);
  const [roomReadPositions] = useTable(tables.roomReadPosition);
  const [messageEditHistories] = useTable(tables.messageEditHistory);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // UI state
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [registerName, setRegisterName] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [editingMsgId, setEditingMsgId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [showHistoryFor, setShowHistoryFor] = useState<bigint | null>(null);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60n);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleDateTime, setScheduleDateTime] = useState('');
  const [error, setError] = useState('');
  const [now, setNow] = useState(Date.now());
  const [showMembers, setShowMembers] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Save token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe to all tables
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM message_read',
        'SELECT * FROM room_read_position',
        'SELECT * FROM message_reaction',
        'SELECT * FROM message_edit_history',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  // Activity heartbeat
  useEffect(() => {
    if (!conn || !isActive || !subscribed) return;
    const iv = setInterval(() => {
      conn.reducers.updateActivity({});
    }, 60_000);
    return () => clearInterval(iv);
  }, [conn, isActive, subscribed]);

  // Realtime clock for ephemeral countdowns
  useEffect(() => {
    const iv = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(iv);
  }, []);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // Auto-mark room as read when selecting a room or new messages arrive
  useEffect(() => {
    if (!selectedRoomId || !conn || !subscribed || !myIdentity) return;
    conn.reducers.markRoomRead({ roomId: selectedRoomId });
  }, [selectedRoomId, messages.length]);

  const myUser = users.find(u => u.identity.toHexString() === myIdentity?.toHexString());
  const myHex = myIdentity?.toHexString();

  // Helper: get user by identity hex
  const getUserByHex = useCallback((hex: string) =>
    users.find(u => u.identity.toHexString() === hex), [users]);

  // Helper: get unread count for a room
  const getUnreadCount = useCallback((roomId: bigint): number => {
    const myPos = roomReadPositions.find(p =>
      p.roomId === roomId && p.identity.toHexString() === myHex
    );
    const lastRead = myPos?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastRead && !m.isDeleted).length;
  }, [roomReadPositions, messages, myHex]);

  // Helper: get typing users in selected room
  const typingUsers = typingIndicators.filter(ind => {
    if (ind.roomId !== selectedRoomId) return false;
    if (ind.identity.toHexString() === myHex) return false;
    return ind.expiresAtMicros > BigInt(now) * 1000n;
  });

  // Helper: am I a member of a room?
  const isMemberOf = useCallback((roomId: bigint): boolean =>
    roomMembers.some(m => m.roomId === roomId && m.identity.toHexString() === myHex && !m.isBanned),
    [roomMembers, myHex]
  );

  // Helper: am I an admin of a room?
  const isAdminOf = useCallback((roomId: bigint): boolean =>
    roomMembers.some(m => m.roomId === roomId && m.identity.toHexString() === myHex && m.isAdmin && !m.isBanned),
    [roomMembers, myHex]
  );

  // Get seen-by list for a message
  const getSeenBy = useCallback((messageId: bigint): string[] => {
    return messageReads
      .filter(r => r.messageId === messageId && r.identity.toHexString() !== myHex)
      .map(r => getUserByHex(r.identity.toHexString())?.name ?? r.identity.toHexString().slice(0, 8));
  }, [messageReads, myHex, getUserByHex]);

  // Handle registration
  const handleRegister = () => {
    if (!conn || !registerName.trim()) return;
    try {
      conn.reducers.register({ name: registerName.trim() });
      setError('');
    } catch (e: any) {
      setError(e.message ?? 'Registration failed');
    }
  };

  // Handle sending a message
  const handleSend = () => {
    if (!conn || !selectedRoomId || !messageText.trim()) return;
    try {
      conn.reducers.sendMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        isEphemeral,
        ephemeralDurationSecs: isEphemeral ? ephemeralDuration : 0n,
      });
      setMessageText('');
      setIsEphemeral(false);
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
    } catch (e: any) {
      setError(e.message ?? 'Send failed');
    }
  };

  // Handle typing
  const handleTypingInput = (val: string) => {
    setMessageText(val);
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: true });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn?.reducers.setTyping({ roomId: selectedRoomId!, isTyping: false });
    }, 4000);
  };

  // Handle schedule submit
  const handleSchedule = () => {
    if (!conn || !selectedRoomId || !messageText.trim() || !scheduleDateTime) return;
    const ms = new Date(scheduleDateTime).getTime();
    if (isNaN(ms)) { setError('Invalid date'); return; }
    const micros = BigInt(ms) * 1000n;
    try {
      conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageText.trim(), sendAtMicros: micros });
      setMessageText('');
      setShowSchedule(false);
      setScheduleDateTime('');
    } catch (e: any) {
      setError(e.message ?? 'Schedule failed');
    }
  };

  // Handle edit submit
  const handleEditSubmit = (msgId: bigint) => {
    if (!conn || !editText.trim()) return;
    conn.reducers.editMessage({ messageId: msgId, newText: editText.trim() });
    setEditingMsgId(null);
    setEditText('');
  };

  // Registration screen
  if (!subscribed) {
    return (
      <div className="loading">
        <div className="loading-text">Connecting to SpacetimeDB Chat...</div>
      </div>
    );
  }

  if (!myUser) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <h1>SpacetimeDB Chat</h1>
          <p>Choose your display name to get started</p>
          {error && <div className="error-msg">{error}</div>}
          <input
            className="text-input"
            placeholder="Your name..."
            value={registerName}
            onChange={e => setRegisterName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            maxLength={32}
            autoFocus
          />
          <button className="btn btn-primary" onClick={handleRegister}>Join Chat</button>
        </div>
      </div>
    );
  }

  // Rooms the user is a member of
  const myRooms = rooms.filter(r => isMemberOf(r.id));
  const otherRooms = rooms.filter(r => !isMemberOf(r.id));

  // Current room messages
  const roomMessages = selectedRoomId
    ? [...messages.filter(m => m.roomId === selectedRoomId)].sort((a, b) => (a.id < b.id ? -1 : 1))
    : [];

  // Current room members
  const currentMembers = selectedRoomId
    ? roomMembers.filter(m => m.roomId === selectedRoomId && !m.isBanned)
    : [];

  // My scheduled messages for this room
  const myScheduled = scheduledMessages.filter(s =>
    s.roomId === selectedRoomId && s.sender.toHexString() === myHex
  );

  const selectedRoom = selectedRoomId ? rooms.find(r => r.id === selectedRoomId) : null;
  const amAdmin = selectedRoomId ? isAdminOf(selectedRoomId) : false;

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        {/* Current user */}
        <div className="sidebar-header">
          <h2 className="app-title">SpacetimeDB Chat</h2>
          <div className="my-profile">
            <span className="status-dot">{statusIcon(myUser.status, myUser.online)}</span>
            <span className="my-name">{myUser.name}</span>
            <select
              className="status-select"
              value={myUser.status}
              onChange={e => conn?.reducers.setStatus({ status: e.target.value })}
            >
              <option value="online">Online</option>
              <option value="away">Away</option>
              <option value="dnd">Do Not Disturb</option>
              <option value="invisible">Invisible</option>
            </select>
          </div>
        </div>

        {/* Rooms */}
        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="btn-icon" onClick={() => setShowCreateRoom(!showCreateRoom)} title="Create room">+</button>
          </div>
          {showCreateRoom && (
            <div className="create-room-form">
              <input
                className="text-input text-input-sm"
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
                maxLength={64}
              />
            </div>
          )}
          <div className="room-list">
            {myRooms.map(r => {
              const unread = getUnreadCount(r.id);
              return (
                <div
                  key={r.id.toString()}
                  className={`room-item ${selectedRoomId === r.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoomId(r.id)}
                >
                  <span className="room-name"># {r.name}</span>
                  {unread > 0 && <span className="unread-badge">{unread}</span>}
                </div>
              );
            })}
          </div>
          {otherRooms.length > 0 && (
            <div className="other-rooms">
              <div className="subsection-label">Other Rooms</div>
              {otherRooms.map(r => (
                <div
                  key={r.id.toString()}
                  className="room-item joinable"
                  onClick={() => {
                    conn?.reducers.joinRoom({ roomId: r.id });
                    setSelectedRoomId(r.id);
                  }}
                >
                  <span className="room-name"># {r.name}</span>
                  <span className="join-hint">Join</span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Online users */}
        <div className="sidebar-section users-section">
          <div className="section-header"><span>Users ({users.filter(u => u.online && u.status !== 'invisible').length} online)</span></div>
          <div className="user-list">
            {[...users].sort((a, b) => {
              if (a.online && !b.online) return -1;
              if (!a.online && b.online) return 1;
              return a.name.localeCompare(b.name);
            }).map(u => (
              <div key={u.identity.toHexString()} className="user-item">
                <span className="status-dot">{statusIcon(u.status, u.online)}</span>
                <span className={`user-name ${!u.online ? 'offline' : ''}`}>{u.name}</span>
                {!u.online && (
                  <span className="last-active">{timeAgo(u.lastActive.microsSinceUnixEpoch)}</span>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Main panel */}
      <div className="main">
        {!selectedRoom ? (
          <div className="no-room">
            <div className="no-room-text">Select or create a room to start chatting</div>
          </div>
        ) : (
          <>
            {/* Room header */}
            <div className="room-header">
              <div className="room-header-left">
                <span className="room-header-name"># {selectedRoom.name}</span>
                {amAdmin && <span className="admin-badge">Admin</span>}
              </div>
              <div className="room-header-right">
                <button className="btn-sm" onClick={() => setShowMembers(!showMembers)}>
                  Members ({currentMembers.length})
                </button>
                <button className="btn-sm danger" onClick={() => {
                  conn?.reducers.leaveRoom({ roomId: selectedRoomId! });
                  setSelectedRoomId(null);
                }}>Leave</button>
              </div>
            </div>

            <div className="room-body">
              {/* Members panel */}
              {showMembers && (
                <div className="members-panel">
                  <div className="members-title">Members</div>
                  {currentMembers.map(m => {
                    const u = getUserByHex(m.identity.toHexString());
                    return (
                      <div key={m.id.toString()} className="member-item">
                        <span>{statusIcon(u?.status ?? 'offline', u?.online ?? false)}</span>
                        <span className="member-name">{u?.name ?? m.identity.toHexString().slice(0, 8)}</span>
                        {m.isAdmin && <span className="admin-tag">admin</span>}
                        {amAdmin && m.identity.toHexString() !== myHex && (
                          <div className="member-actions">
                            <button className="btn-xs" onClick={() => conn?.reducers.kickUser({ roomId: selectedRoomId!, targetIdentity: m.identity })}>Kick</button>
                            <button className="btn-xs danger" onClick={() => conn?.reducers.banUser({ roomId: selectedRoomId!, targetIdentity: m.identity })}>Ban</button>
                            {!m.isAdmin && <button className="btn-xs accent" onClick={() => conn?.reducers.promoteUser({ roomId: selectedRoomId!, targetIdentity: m.identity })}>Promote</button>}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              {/* Messages */}
              <div className="messages-area">
                <div className="messages-scroll">
                  {roomMessages.length === 0 && (
                    <div className="no-messages">No messages yet. Say hello!</div>
                  )}
                  {roomMessages.map(msg => {
                    if (msg.isDeleted && !msg.isEphemeral) {
                      return null; // Hide permanently deleted messages
                    }
                    const sender = getUserByHex(msg.sender.toHexString());
                    const isMe = msg.sender.toHexString() === myHex;
                    const reactions = messageReactions.filter(r => r.messageId === msg.id);
                    const seenBy = getSeenBy(msg.id);
                    const history = messageEditHistories.filter(h => h.messageId === msg.id).sort((a, b) => (a.id < b.id ? -1 : 1));

                    // Ephemeral countdown
                    let ephemeralRemaining: number | null = null;
                    if (msg.isEphemeral && !msg.isDeleted && msg.ephemeralDurationSecs > 0n) {
                      const sentMs = Number(msg.sentAt.microsSinceUnixEpoch / 1000n);
                      const expiresMs = sentMs + Number(msg.ephemeralDurationSecs) * 1000;
                      ephemeralRemaining = Math.max(0, Math.floor((expiresMs - now) / 1000));
                    }

                    return (
                      <div key={msg.id.toString()} className={`message ${isMe ? 'mine' : ''} ${msg.isDeleted ? 'deleted' : ''}`}>
                        <div className="msg-header">
                          <span className="msg-sender">{sender?.name ?? 'Unknown'}</span>
                          <span className="msg-time">{formatTime(msg.sentAt.microsSinceUnixEpoch)}</span>
                          {msg.isEphemeral && ephemeralRemaining !== null && (
                            <span className="ephemeral-badge" title="Ephemeral message">
                              ⏱ {ephemeralRemaining > 0 ? `${ephemeralRemaining}s` : 'Expired'}
                            </span>
                          )}
                        </div>

                        {editingMsgId === msg.id ? (
                          <div className="edit-form">
                            <input
                              className="text-input"
                              value={editText}
                              onChange={e => setEditText(e.target.value)}
                              onKeyDown={e => {
                                if (e.key === 'Enter') handleEditSubmit(msg.id);
                                if (e.key === 'Escape') { setEditingMsgId(null); setEditText(''); }
                              }}
                              autoFocus
                            />
                            <button className="btn-xs" onClick={() => handleEditSubmit(msg.id)}>Save</button>
                            <button className="btn-xs" onClick={() => { setEditingMsgId(null); setEditText(''); }}>Cancel</button>
                          </div>
                        ) : (
                          <div className="msg-text">
                            {msg.isDeleted ? <em className="deleted-text">[Message expired]</em> : msg.text}
                            {msg.isEdited && !msg.isDeleted && <span className="edited-tag">(edited)</span>}
                          </div>
                        )}

                        {/* Reactions */}
                        {!msg.isDeleted && (
                          <div className="reactions-row">
                            {EMOJIS.map(emoji => {
                              const reactors = reactions.filter(r => r.emoji === emoji);
                              const iReacted = reactors.some(r => r.identity.toHexString() === myHex);
                              return reactors.length > 0 ? (
                                <button
                                  key={emoji}
                                  className={`reaction-btn ${iReacted ? 'reacted' : ''}`}
                                  title={reactors.map(r => getUserByHex(r.identity.toHexString())?.name ?? '?').join(', ')}
                                  onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                                >
                                  {emoji} {reactors.length}
                                </button>
                              ) : null;
                            })}
                            {!msg.isDeleted && (
                              <div className="emoji-picker-inline">
                                {EMOJIS.map(emoji => (
                                  <button
                                    key={emoji}
                                    className="emoji-add-btn"
                                    onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                                    title={`React with ${emoji}`}
                                  >
                                    {emoji}
                                  </button>
                                ))}
                              </div>
                            )}
                          </div>
                        )}

                        {/* Message actions */}
                        {isMe && !msg.isDeleted && (
                          <div className="msg-actions">
                            <button className="btn-xs" onClick={() => {
                              setEditingMsgId(msg.id);
                              setEditText(msg.text);
                            }}>Edit</button>
                            {history.length > 0 && (
                              <button className="btn-xs" onClick={() => setShowHistoryFor(showHistoryFor === msg.id ? null : msg.id)}>
                                History ({history.length})
                              </button>
                            )}
                          </div>
                        )}

                        {/* Edit history */}
                        {showHistoryFor === msg.id && history.length > 0 && (
                          <div className="edit-history">
                            <div className="history-title">Edit History</div>
                            {history.map(h => (
                              <div key={h.id.toString()} className="history-item">
                                <span className="history-text">{h.text}</span>
                                <span className="history-time">{formatDate(h.editedAt.microsSinceUnixEpoch)}</span>
                              </div>
                            ))}
                          </div>
                        )}

                        {/* Seen by */}
                        {isMe && seenBy.length > 0 && (
                          <div className="seen-by">Seen by {seenBy.join(', ')}</div>
                        )}
                      </div>
                    );
                  })}

                  {/* Typing indicator */}
                  {typingUsers.length > 0 && (
                    <div className="typing-indicator">
                      {typingUsers.length === 1
                        ? `${getUserByHex(typingUsers[0].identity.toHexString())?.name ?? 'Someone'} is typing...`
                        : `${typingUsers.length} people are typing...`}
                    </div>
                  )}

                  <div ref={messagesEndRef} />
                </div>

                {/* Scheduled messages for this room (mine) */}
                {myScheduled.length > 0 && (
                  <div className="scheduled-section">
                    <div className="scheduled-title">Scheduled Messages</div>
                    {myScheduled.map(s => {
                      let timeStr = '';
                      if (s.scheduledAt.tag === 'Time') {
                        timeStr = formatDate(s.scheduledAt.value.microsSinceUnixEpoch);
                      }
                      return (
                        <div key={s.scheduledId.toString()} className="scheduled-item">
                          <span className="scheduled-text">{s.text}</span>
                          <span className="scheduled-time">@ {timeStr}</span>
                          <button
                            className="btn-xs danger"
                            onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: s.scheduledId })}
                          >Cancel</button>
                        </div>
                      );
                    })}
                  </div>
                )}

                {/* Message input */}
                <div className="input-area">
                  {error && <div className="error-msg" onClick={() => setError('')}>{error} ✕</div>}
                  <div className="input-options">
                    <label className="ephemeral-toggle">
                      <input
                        type="checkbox"
                        checked={isEphemeral}
                        onChange={e => setIsEphemeral(e.target.checked)}
                      />
                      <span>Ephemeral</span>
                    </label>
                    {isEphemeral && (
                      <select
                        className="duration-select"
                        value={ephemeralDuration.toString()}
                        onChange={e => setEphemeralDuration(BigInt(e.target.value))}
                      >
                        {EPHEMERAL_OPTIONS.map(o => (
                          <option key={o.label} value={o.secs.toString()}>{o.label}</option>
                        ))}
                      </select>
                    )}
                    <button
                      className={`btn-sm ${showSchedule ? 'active' : ''}`}
                      onClick={() => setShowSchedule(!showSchedule)}
                    >⏰ Schedule</button>
                  </div>

                  {showSchedule && (
                    <div className="schedule-form">
                      <input
                        type="datetime-local"
                        className="text-input"
                        value={scheduleDateTime}
                        onChange={e => setScheduleDateTime(e.target.value)}
                        min={new Date(Date.now() + 10000).toISOString().slice(0, 16)}
                      />
                      <button className="btn btn-primary" onClick={handleSchedule}>Schedule Message</button>
                    </div>
                  )}

                  <div className="input-row">
                    <textarea
                      className="message-input"
                      placeholder={isEphemeral ? `Ephemeral message (${EPHEMERAL_OPTIONS.find(o => o.secs === ephemeralDuration)?.label})...` : 'Message...'}
                      value={messageText}
                      onChange={e => handleTypingInput(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === 'Enter' && !e.shiftKey) {
                          e.preventDefault();
                          if (showSchedule) handleSchedule();
                          else handleSend();
                        }
                      }}
                      onBlur={() => {
                        if (selectedRoomId && conn) {
                          conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
                          if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
                        }
                      }}
                      rows={2}
                    />
                    <button
                      className="btn btn-primary send-btn"
                      onClick={showSchedule ? handleSchedule : handleSend}
                      disabled={!messageText.trim()}
                    >
                      {showSchedule ? '⏰' : '→'}
                    </button>
                  </div>
                </div>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
