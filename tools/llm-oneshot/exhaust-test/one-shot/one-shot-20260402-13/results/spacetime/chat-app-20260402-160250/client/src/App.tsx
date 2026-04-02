import { useState, useEffect, useCallback, useRef } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

type DbConn = DbConnection;

// ── Helpers ────────────────────────────────────────────────────────────────

function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  return tsToDate(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatRelative(ts: { microsSinceUnixEpoch: bigint }): string {
  const diffMs = Date.now() - Number(ts.microsSinceUnixEpoch / 1000n);
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function idToStr(id: { toHexString(): string }): string {
  return id.toHexString();
}

function getInitials(name: string): string {
  return name.split(' ').map(w => w[0]).join('').toUpperCase().slice(0, 2);
}

const STATUS_LABELS: Record<string, string> = {
  online: 'Online', away: 'Away', dnd: 'Do Not Disturb', invisible: 'Invisible',
};

// ── Auth Screen ────────────────────────────────────────────────────────────

function AuthScreen({ onRegister }: { onRegister: (name: string) => void }) {
  const [name, setName] = useState('');
  const [err, setErr] = useState('');

  function submit() {
    const t = name.trim();
    if (!t) { setErr('Name required'); return; }
    if (t.length > 32) { setErr('Max 32 chars'); return; }
    onRegister(t);
  }

  return (
    <div className="auth-screen">
      <div className="auth-box">
        <h1>SpacetimeDB Chat</h1>
        <p>Enter a display name to join</p>
        <input
          value={name} onChange={e => setName(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && submit()}
          placeholder="Your name" maxLength={32} autoFocus
        />
        {err && <div className="error-msg">{err}</div>}
        <button className="btn btn-primary" style={{ width: '100%' }} onClick={submit}>Join</button>
      </div>
    </div>
  );
}

// ── Edit History Modal ─────────────────────────────────────────────────────

function EditHistoryModal({ messageId, messageEdits, onClose }: {
  messageId: bigint;
  messageEdits: ReturnType<typeof useTable>[0];
  onClose: () => void;
}) {
  const edits = [...messageEdits].filter(e => e.messageId === messageId).sort(
    (a, b) => Number(b.editedAt.microsSinceUnixEpoch - a.editedAt.microsSinceUnixEpoch)
  );
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Edit History</h3>
        {edits.length === 0 ? <p style={{ color: 'var(--text-muted)' }}>No edit history</p> : (
          <div className="edit-history">
            {edits.map(e => (
              <div key={String(e.id)} className="edit-entry">
                <span style={{ color: 'var(--text-muted)', fontSize: '0.72rem' }}>{tsToDate(e.editedAt).toLocaleString()}: </span>
                {e.oldText}
              </div>
            ))}
          </div>
        )}
        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

// ── Admin Panel ────────────────────────────────────────────────────────────

function AdminPanel({ roomId, members, users, conn, myIdentity, onClose }: {
  roomId: bigint;
  members: ReturnType<typeof useTable>[0];
  users: ReturnType<typeof useTable>[0];
  conn: DbConn;
  myIdentity: { toHexString(): string };
  onClose: () => void;
}) {
  const roomMembers = [...members].filter(m => m.roomId === roomId && !m.isBanned);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Room Members</h3>
        <div className="admin-panel">
          {roomMembers.map(m => {
            const user = [...users].find(u => idToStr(u.identity) === idToStr(m.identity));
            const isMe = idToStr(m.identity) === idToStr(myIdentity);
            return (
              <div key={idToStr(m.identity)} className="member-item">
                <span>{user?.name ?? 'Unknown'} {m.isAdmin ? '(admin)' : ''} {isMe ? '(you)' : ''}</span>
                {!isMe && (
                  <div className="member-actions">
                    <button className="btn btn-sm btn-secondary"
                      onClick={() => conn.reducers.kickUser({ roomId, targetIdentity: m.identity })}>
                      Kick
                    </button>
                    <button className="btn btn-sm btn-danger"
                      onClick={() => conn.reducers.banUser({ roomId, targetIdentity: m.identity })}>
                      Ban
                    </button>
                    {!m.isAdmin && (
                      <button className="btn btn-sm btn-secondary"
                        onClick={() => conn.reducers.promoteUser({ roomId, targetIdentity: m.identity })}>
                        Promote
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

// ── Message Component ──────────────────────────────────────────────────────

function MessageItem({
  msg, isOwn, senderName, reactions, readReceipts, messageEdits, allUsers, conn, myIdentity,
  onEditRequest,
}: {
  msg: { id: bigint; roomId: bigint; sender: { toHexString(): string }; text: string; sentAt: { microsSinceUnixEpoch: bigint }; edited: boolean; ephemeralExpiry?: { microsSinceUnixEpoch: bigint } | null };
  isOwn: boolean;
  senderName: string;
  reactions: ReturnType<typeof useTable>[0];
  readReceipts: ReturnType<typeof useTable>[0];
  messageEdits: ReturnType<typeof useTable>[0];
  allUsers: ReturnType<typeof useTable>[0];
  conn: DbConn;
  myIdentity: { toHexString(): string };
  onEditRequest: (msgId: bigint, text: string) => void;
}) {
  const [showHistory, setShowHistory] = useState(false);
  const msgReactions = [...reactions].filter(r => r.messageId === msg.id);

  // Group reactions by emoji
  const reactionMap = new Map<string, { count: number; mine: boolean; identities: string[] }>();
  for (const r of msgReactions) {
    const key = r.emoji;
    const existing = reactionMap.get(key) ?? { count: 0, mine: false, identities: [] };
    existing.count++;
    existing.identities.push(idToStr(r.identity));
    if (idToStr(r.identity) === idToStr(myIdentity)) existing.mine = true;
    reactionMap.set(key, existing);
  }

  // Read receipts for this message
  const seenBy = [...readReceipts]
    .filter(rr => rr.lastReadMessageId >= msg.id && idToStr(rr.identity) !== idToStr(myIdentity))
    .map(rr => {
      const u = [...allUsers].find(u => idToStr(u.identity) === idToStr(rr.identity));
      return u?.name ?? 'Unknown';
    });

  const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

  // Ephemeral countdown
  const [countdown, setCountdown] = useState<string | null>(null);
  useEffect(() => {
    if (!msg.ephemeralExpiry) return;
    const expiry = Number(msg.ephemeralExpiry.microsSinceUnixEpoch / 1000n);
    const tick = () => {
      const remaining = expiry - Date.now();
      if (remaining <= 0) { setCountdown('expired'); return; }
      const secs = Math.ceil(remaining / 1000);
      setCountdown(`${secs}s`);
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [msg.ephemeralExpiry]);

  return (
    <div className={`message ${isOwn ? 'own' : ''}`}>
      <div className="message-avatar">{getInitials(senderName)}</div>
      <div className="message-content">
        <div className="message-header">
          <span className="message-sender">{senderName}</span>
          <span className="message-time">{formatTime(msg.sentAt)}</span>
          {msg.edited && <span className="message-edited">(edited)</span>}
          {countdown && <span className="ephemeral-indicator">⏱ {countdown}</span>}
        </div>
        <div className="message-text">{msg.text}</div>
        {reactionMap.size > 0 && (
          <div className="reactions">
            {[...reactionMap.entries()].map(([emoji, data]) => (
              <button key={emoji} className={`reaction-chip ${data.mine ? 'mine' : ''}`}
                title={data.identities.join(', ')}
                onClick={() => conn.reducers.toggleReaction({ messageId: msg.id, emoji })}>
                {emoji} <span className="count">{data.count}</span>
              </button>
            ))}
          </div>
        )}
        {isOwn && seenBy.length > 0 && (
          <div className="read-receipt">Seen by {seenBy.join(', ')}</div>
        )}
        <div className="message-actions">
          {EMOJIS.map(e => (
            <button key={e} className="action-btn" title={`React ${e}`}
              onClick={() => conn.reducers.toggleReaction({ messageId: msg.id, emoji: e })}>
              {e}
            </button>
          ))}
          {isOwn && (
            <button className="action-btn" title="Edit" onClick={() => onEditRequest(msg.id, msg.text)}>✏️</button>
          )}
          {msg.edited && (
            <button className="action-btn" title="View history" onClick={() => setShowHistory(true)}>📋</button>
          )}
        </div>
      </div>
      {showHistory && (
        <EditHistoryModal messageId={msg.id} messageEdits={messageEdits} onClose={() => setShowHistory(false)} />
      )}
    </div>
  );
}

// ── Main App ───────────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConn | null;

  const [subscribed, setSubscribed] = useState(false);
  const [registered, setRegistered] = useState(false);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [editingMsgId, setEditingMsgId] = useState<bigint | null>(null);
  const [ephemeralTtl, setEphemeralTtl] = useState<number>(0); // 0 = not ephemeral
  const [scheduleTime, setScheduleTime] = useState('');
  const [showAdmin, setShowAdmin] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM reaction',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [messageEdits] = useTable(tables.messageEdit);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [reactions] = useTable(tables.reaction);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  const myUser = myIdentity ? [...users].find(u => idToStr(u.identity) === idToStr(myIdentity)) : null;

  // Check registration
  useEffect(() => {
    if (subscribed && myIdentity) {
      const user = [...users].find(u => idToStr(u.identity) === idToStr(myIdentity));
      setRegistered(!!user);
    }
  }, [subscribed, myIdentity, users]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // Mark read when viewing a room
  useEffect(() => {
    if (!conn || !selectedRoomId || !myIdentity) return;
    const roomMsgs = [...messages].filter(m => m.roomId === selectedRoomId);
    if (roomMsgs.length === 0) return;
    const lastMsg = roomMsgs.reduce((a, b) => a.id > b.id ? a : b);
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [messages, selectedRoomId, conn, myIdentity]);

  // Expire typing indicators on client (purely visual)
  const [, forceUpdate] = useState(0);
  useEffect(() => {
    const id = setInterval(() => forceUpdate(n => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // Joined rooms
  const myRoomIds = myIdentity
    ? new Set([...roomMembers].filter(m => idToStr(m.identity) === idToStr(myIdentity) && !m.isBanned).map(m => m.roomId))
    : new Set<bigint>();

  const joinedRooms = [...rooms].filter(r => myRoomIds.has(r.id));
  const availableRooms = [...rooms].filter(r => !myRoomIds.has(r.id));

  // Unread counts
  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const myReceipt = [...readReceipts].find(rr => rr.roomId === roomId && idToStr(rr.identity) === idToStr(myIdentity));
    const lastReadId = myReceipt?.lastReadMessageId ?? 0n;
    return [...messages].filter(m => m.roomId === roomId && m.id > lastReadId).length;
  }

  // Typing users in selected room
  const now = Date.now();
  const typingUsers = selectedRoomId
    ? [...typingIndicators].filter(ti =>
        ti.roomId === selectedRoomId &&
        idToStr(ti.identity) !== (myIdentity ? idToStr(myIdentity) : '') &&
        Number(ti.expiresAt.microsSinceUnixEpoch / 1000n) > now
      ).map(ti => {
        const u = [...users].find(u => idToStr(u.identity) === idToStr(ti.identity));
        return u?.name ?? 'Someone';
      })
    : [];

  // Room messages
  const roomMessages = selectedRoomId
    ? [...messages].filter(m => m.roomId === selectedRoomId).sort((a, b) => Number(a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch))
    : [];

  // My scheduled messages for selected room
  const myScheduled = myIdentity && selectedRoomId
    ? [...scheduledMessages].filter(sm => idToStr(sm.author) === idToStr(myIdentity) && sm.roomId === selectedRoomId)
    : [];

  // Is admin in selected room
  const isAdmin = myIdentity && selectedRoomId
    ? [...roomMembers].some(m => m.roomId === selectedRoomId && idToStr(m.identity) === idToStr(myIdentity) && m.isAdmin)
    : false;

  const handleRegister = useCallback((name: string) => {
    conn?.reducers.register({ name });
  }, [conn]);

  const handleTyping = useCallback(() => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.clearTyping({ roomId: selectedRoomId });
    }, 4000);
  }, [conn, selectedRoomId]);

  const handleSend = useCallback(() => {
    if (!conn || !selectedRoomId || !messageText.trim()) return;

    if (editingMsgId !== null) {
      conn.reducers.editMessage({ messageId: editingMsgId, newText: messageText.trim() });
      setEditingMsgId(null);
    } else if (scheduleTime) {
      const sendAtMs = new Date(scheduleTime).getTime();
      const sendAtMicros = BigInt(sendAtMs) * 1000n;
      conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageText.trim(), sendAtMicros });
      setScheduleTime('');
    } else if (ephemeralTtl > 0) {
      conn.reducers.sendEphemeralMessage({ roomId: selectedRoomId, text: messageText.trim(), ttlSeconds: ephemeralTtl });
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    }

    setMessageText('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    conn.reducers.clearTyping({ roomId: selectedRoomId });
  }, [conn, selectedRoomId, messageText, editingMsgId, scheduleTime, ephemeralTtl]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  if (!isActive || !subscribed) {
    return (
      <div className="auth-screen">
        <div className="auth-box">
          <h1>SpacetimeDB Chat</h1>
          <p style={{ color: 'var(--text-muted)' }}>Connecting...</p>
        </div>
      </div>
    );
  }

  if (!registered) {
    return <AuthScreen onRegister={handleRegister} />;
  }

  return (
    <div className="app">
      <div className="layout">
        {/* Sidebar */}
        <div className="sidebar">
          <div className="sidebar-header">SpacetimeDB Chat</div>

          {/* Joined Rooms */}
          <div className="sidebar-section">
            <h3>My Rooms</h3>
            {joinedRooms.map(r => {
              const unread = getUnreadCount(r.id);
              return (
                <div key={String(r.id)} className={`room-item ${selectedRoomId === r.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoomId(r.id)}>
                  <span className="room-name"># {r.name}</span>
                  {unread > 0 && <span className="unread-badge">{unread}</span>}
                </div>
              );
            })}

            {/* Available Rooms */}
            {availableRooms.length > 0 && (
              <>
                <h3 style={{ marginTop: '8px' }}>Available Rooms</h3>
                {availableRooms.map(r => (
                  <div key={String(r.id)} className="room-item" style={{ opacity: 0.6 }}>
                    <span className="room-name"># {r.name}</span>
                    <button className="btn btn-sm btn-secondary"
                      onClick={() => conn?.reducers.joinRoom({ roomId: r.id })}>
                      Join
                    </button>
                  </div>
                ))}
              </>
            )}
          </div>

          {/* Create Room */}
          <div className="create-room-form">
            <input
              value={newRoomName} onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter' && newRoomName.trim()) {
                  conn?.reducers.createRoom({ name: newRoomName.trim() });
                  setNewRoomName('');
                }
              }}
              placeholder="New room name..." maxLength={64}
            />
            <button className="btn btn-primary" style={{ width: '100%' }}
              onClick={() => {
                if (newRoomName.trim()) {
                  conn?.reducers.createRoom({ name: newRoomName.trim() });
                  setNewRoomName('');
                }
              }}>
              + Create Room
            </button>
          </div>

          {/* Status selector */}
          <div className="status-selector">
            <select value={myUser?.status ?? 'online'} onChange={e => conn?.reducers.setStatus({ status: e.target.value })}>
              {Object.entries(STATUS_LABELS).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
            </select>
          </div>

          {/* Online Users */}
          <div className="user-list">
            <h3>Users</h3>
            {[...users].sort((a, b) => {
              const ao = a.online && a.status !== 'invisible' ? 0 : 1;
              const bo = b.online && b.status !== 'invisible' ? 0 : 1;
              return ao - bo || a.name.localeCompare(b.name);
            }).map(u => {
              const isMe = idToStr(u.identity) === (myIdentity ? idToStr(myIdentity) : '');
              const statusClass = u.online ? (u.status === 'invisible' ? 'offline' : u.status) : 'offline';
              return (
                <div key={idToStr(u.identity)} className="user-item">
                  <div className="user-avatar">
                    {getInitials(u.name)}
                    <span className={`status-dot ${statusClass}`} />
                  </div>
                  <div>
                    <div className="user-name-text">{u.name}{isMe ? ' (you)' : ''}</div>
                    {!u.online && (
                      <div className="user-status-text">Last active {formatRelative(u.lastActive)}</div>
                    )}
                    {u.online && u.status !== 'online' && (
                      <div className="user-status-text">{STATUS_LABELS[u.status] ?? u.status}</div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Chat Area */}
        <div className="chat-area">
          {!selectedRoomId ? (
            <div className="no-room">Select a room to start chatting</div>
          ) : (
            <>
              <div className="chat-header">
                <h2># {[...rooms].find(r => r.id === selectedRoomId)?.name ?? ''}</h2>
                <div className="chat-header-actions">
                  {isAdmin && (
                    <button className="btn btn-secondary btn-sm" onClick={() => setShowAdmin(true)}>Manage</button>
                  )}
                  <button className="btn btn-secondary btn-sm"
                    onClick={() => { conn?.reducers.leaveRoom({ roomId: selectedRoomId }); setSelectedRoomId(null); }}>
                    Leave
                  </button>
                </div>
              </div>

              {/* Scheduled Messages */}
              {myScheduled.length > 0 && (
                <div className="scheduled-panel">
                  <h4>Scheduled Messages ({myScheduled.length})</h4>
                  {myScheduled.map(sm => {
                    const sendAtMicros = sm.scheduledAt.tag === 'Time' ? sm.scheduledAt.value.microsSinceUnixEpoch : 0n;
                    const sendAt = new Date(Number(sendAtMicros / 1000n));
                    return (
                      <div key={String(sm.scheduledId)} className="scheduled-item">
                        <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{sm.text}</span>
                        <span style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginLeft: '8px' }}>
                          {sendAt.toLocaleString()}
                        </span>
                        <button className="btn btn-sm btn-danger" style={{ marginLeft: '8px' }}
                          onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: sm.scheduledId })}>
                          Cancel
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="messages-container">
                {roomMessages.map(msg => {
                  const isOwn = myIdentity ? idToStr(msg.sender) === idToStr(myIdentity) : false;
                  const senderUser = [...users].find(u => idToStr(u.identity) === idToStr(msg.sender));
                  return (
                    <MessageItem
                      key={String(msg.id)}
                      msg={msg}
                      isOwn={isOwn}
                      senderName={senderUser?.name ?? 'Unknown'}
                      reactions={reactions}
                      readReceipts={[...readReceipts].filter(rr => rr.roomId === selectedRoomId)}
                      messageEdits={messageEdits}
                      allUsers={users}
                      conn={conn!}
                      myIdentity={myIdentity!}
                      onEditRequest={(id, text) => { setEditingMsgId(id); setMessageText(text); }}
                    />
                  );
                })}
                <div ref={messagesEndRef} />
              </div>

              {/* Typing indicator */}
              <div className="typing-area">
                {typingUsers.length > 0 && (
                  <span className="typing-text">
                    {typingUsers.length === 1
                      ? `${typingUsers[0]} is typing...`
                      : `${typingUsers.join(', ')} are typing...`}
                  </span>
                )}
              </div>

              {/* Input */}
              <div className="input-area">
                {editingMsgId !== null && (
                  <div style={{ color: 'var(--primary)', fontSize: '0.8rem', marginBottom: '6px' }}>
                    Editing message —{' '}
                    <button className="action-btn" onClick={() => { setEditingMsgId(null); setMessageText(''); }}>Cancel</button>
                  </div>
                )}
                <div className="message-input-row">
                  <textarea
                    value={messageText}
                    onChange={e => { setMessageText(e.target.value); handleTyping(); }}
                    onKeyDown={handleKeyDown}
                    placeholder={editingMsgId ? 'Edit message...' : scheduleTime ? 'Schedule message...' : ephemeralTtl > 0 ? 'Ephemeral message...' : 'Message... (Enter to send)'}
                    rows={1}
                  />
                  <button className="btn btn-primary" onClick={handleSend}>Send</button>
                </div>
                {!editingMsgId && (
                  <div className="input-options">
                    <label>Disappears in:</label>
                    <select value={ephemeralTtl} onChange={e => { setEphemeralTtl(Number(e.target.value)); if (Number(e.target.value) > 0) setScheduleTime(''); }}>
                      <option value={0}>Never</option>
                      <option value={60}>1 minute</option>
                      <option value={300}>5 minutes</option>
                      <option value={3600}>1 hour</option>
                    </select>
                    <label>Schedule at:</label>
                    <input type="datetime-local"
                      value={scheduleTime}
                      onChange={e => { setScheduleTime(e.target.value); if (e.target.value) setEphemeralTtl(0); }}
                    />
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>

      {showAdmin && selectedRoomId !== null && myIdentity && conn && (
        <AdminPanel
          roomId={selectedRoomId}
          members={roomMembers}
          users={users}
          conn={conn}
          myIdentity={myIdentity}
          onClose={() => setShowAdmin(false)}
        />
      )}
    </div>
  );
}
