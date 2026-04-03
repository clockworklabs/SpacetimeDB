import { useState, useEffect, useRef } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import './styles.css';

// ─── Helpers ──────────────────────────────────────────────────────────────────

const microsToMs = (micros: bigint): number => Number(micros / 1000n);

const formatTime = (micros: bigint): string =>
  new Date(microsToMs(micros)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

const formatRelative = (micros: bigint): string => {
  const diffMs = Date.now() - microsToMs(micros);
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
};

const STATUS_COLOR: Record<string, string> = {
  online: '#4cf490',
  away: '#fbdc8e',
  dnd: '#ff4c4c',
  invisible: '#6f7987',
};

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

// ─── Main component ───────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Save auth token on change
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscription state
  const [subscribed, setSubscribed] = useState(false);
  useEffect(() => {
    if (!conn || !isActive) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM message_edit_history',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM last_read',
        'SELECT * FROM reaction',
        'SELECT * FROM scheduled_message_delivery',
      ]);
  }, [conn, isActive]);

  // Table data
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [editHistories] = useTable(tables.messageEditHistory);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [lastReads] = useTable(tables.lastRead);
  const [reactions] = useTable(tables.reaction);
  const [scheduledDeliveries] = useTable(tables.scheduledMessageDelivery);

  // Current user
  const myHex = myIdentity?.toHexString();
  const myUser = users.find(u => u.identity.toHexString() === myHex);

  // Re-render tick for live countdowns
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  // UI state
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [ephemeralSecs, setEphemeralSecs] = useState(0);
  const [showSchedulePanel, setShowSchedulePanel] = useState(false);
  const [scheduleText, setScheduleText] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [editingMsgId, setEditingMsgId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [historyMsgId, setHistoryMsgId] = useState<bigint | null>(null);
  const [showMembers, setShowMembers] = useState(false);
  const [hoveredMsgId, setHoveredMsgId] = useState<bigint | null>(null);
  const [reactPickerMsgId, setReactPickerMsgId] = useState<bigint | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [kickedNotice, setKickedNotice] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const markReadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const prevMemberRoomsRef = useRef<Set<string>>(new Set());
  const leavingRef = useRef<bigint | null>(null);

  // ── Kick detection ──────────────────────────────────────────────────────────
  useEffect(() => {
    if (!myHex || !subscribed) return;
    const current = new Set(
      roomMembers
        .filter(m => m.userId.toHexString() === myHex && !m.isBanned)
        .map(m => m.roomId.toString())
    );
    if (selectedRoomId !== null) {
      const selStr = selectedRoomId.toString();
      const wasIn = prevMemberRoomsRef.current.has(selStr);
      const isIn = current.has(selStr);
      if (wasIn && !isIn && leavingRef.current !== selectedRoomId) {
        // Kicked or banned
        setSelectedRoomId(null);
        setKickedNotice(true);
        setTimeout(() => setKickedNotice(false), 4000);
      }
    }
    prevMemberRoomsRef.current = current;
  }, [roomMembers, myHex, subscribed, selectedRoomId]);

  // ── Auto-scroll ─────────────────────────────────────────────────────────────
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // ── Mark room as read ───────────────────────────────────────────────────────
  useEffect(() => {
    if (!conn || !selectedRoomId) return;
    const roomMsgs = messages.filter(m => m.roomId === selectedRoomId);
    if (roomMsgs.length === 0) return;
    const latestId = roomMsgs.reduce((max, m) => (m.id > max ? m.id : max), 0n);
    if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    markReadTimerRef.current = setTimeout(() => {
      conn.reducers.markRoomRead({ roomId: selectedRoomId, lastMessageId: latestId });
    }, 400);
  }, [conn, selectedRoomId, messages]);

  // ── Typing handler ──────────────────────────────────────────────────────────
  const fireTyping = () => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.clearTyping({ roomId: selectedRoomId });
    }, 4000);
  };

  // ── Derived data ────────────────────────────────────────────────────────────
  const myMembership = selectedRoomId
    ? roomMembers.find(m => m.roomId === selectedRoomId && m.userId.toHexString() === myHex)
    : null;
  const isAdmin = myMembership?.isAdmin ?? false;

  const getUnreadCount = (roomId: bigint): number => {
    if (!myHex) return 0;
    const lr = lastReads.find(r => r.roomId === roomId && r.userId.toHexString() === myHex);
    const lastId = lr?.lastMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastId).length;
  };

  const typingUsers = selectedRoomId
    ? typingIndicators
        .filter(
          ti =>
            ti.roomId === selectedRoomId &&
            ti.userId.toHexString() !== myHex &&
            ti.expiresAtMicros / 1000n > BigInt(now)
        )
        .map(ti => users.find(u => u.identity.toHexString() === ti.userId.toHexString())?.name ?? 'Someone')
    : [];

  // ── Send message ────────────────────────────────────────────────────────────
  const handleSend = () => {
    if (!conn || !selectedRoomId || !messageText.trim()) return;
    if (ephemeralSecs > 0) {
      conn.reducers.sendEphemeralMessage({
        roomId: selectedRoomId,
        text: messageText,
        durationMicros: BigInt(ephemeralSecs) * 1_000_000n,
      });
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText });
    }
    setMessageText('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
  };

  // ── Loading / connection ────────────────────────────────────────────────────
  if (!isActive || !subscribed) {
    return (
      <div style={S.center}>
        <div style={{ textAlign: 'center' }}>
          <div style={{ fontSize: 28, fontWeight: 700, color: '#4cf490', marginBottom: 8 }}>SpacetimeDB Chat</div>
          <div style={{ color: '#6f7987' }}>Connecting…</div>
        </div>
      </div>
    );
  }

  // ── Registration ────────────────────────────────────────────────────────────
  if (!myUser) {
    return (
      <div style={S.center}>
        <div style={S.card}>
          <h1 style={{ fontSize: 22, fontWeight: 700, color: '#4cf490', marginBottom: 4 }}>SpacetimeDB Chat</h1>
          <p style={{ color: '#6f7987', marginBottom: 20, fontSize: 13 }}>Enter your display name to continue</p>
          <input
            type="text"
            placeholder="Your name"
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && nameInput.trim()) conn?.reducers.register({ name: nameInput });
            }}
            style={S.input}
            autoFocus
          />
          <button
            onClick={() => { if (nameInput.trim()) conn?.reducers.register({ name: nameInput }); }}
            style={{ ...S.btn, marginTop: 10, width: '100%', background: '#4cf490', color: '#0d0d0e', fontWeight: 700 }}
          >
            Join
          </button>
        </div>
      </div>
    );
  }

  // ── Main layout ─────────────────────────────────────────────────────────────
  const roomMessages = selectedRoomId
    ? [...messages.filter(m => m.roomId === selectedRoomId)].sort((a, b) =>
        a.sentAtMicros < b.sentAtMicros ? -1 : 1
      )
    : [];

  return (
    <div style={{ display: 'flex', height: '100vh', background: '#0d0d0e', color: '#e6e9f0', overflow: 'hidden' }}>
      {/* ── Sidebar ── */}
      <div style={S.sidebar}>
        {/* Title */}
        <div style={{ padding: '14px 16px', borderBottom: '1px solid #202126' }}>
          <div style={{ fontSize: 15, fontWeight: 700, background: 'linear-gradient(266deg,#4cf490 0%,#8a38f5 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
            SpacetimeDB Chat
          </div>
        </div>

        {/* My info + status */}
        <div style={{ padding: '10px 16px', borderBottom: '1px solid #202126' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 7 }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: STATUS_COLOR[myUser.status] ?? '#6f7987', display: 'inline-block', flexShrink: 0 }} />
            <span style={{ fontWeight: 600, fontSize: 13 }}>{myUser.name}</span>
          </div>
          <select
            value={myUser.status}
            onChange={e => conn?.reducers.setStatus({ status: e.target.value })}
            style={{ ...S.input, padding: '4px 8px', fontSize: 12 }}
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        {/* Rooms */}
        <div style={{ flex: 1, overflow: 'auto' }}>
          <div style={{ padding: '8px 16px 4px', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span style={S.sectionLabel}>Rooms</span>
            <button onClick={() => setShowCreateRoom(v => !v)} style={S.iconBtn} title="New room">+</button>
          </div>

          {showCreateRoom && (
            <div style={{ padding: '0 10px 8px' }}>
              <input
                type="text"
                placeholder="Room name"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter' && newRoomName.trim()) {
                    conn?.reducers.createRoom({ name: newRoomName });
                    setNewRoomName('');
                    setShowCreateRoom(false);
                  }
                  if (e.key === 'Escape') setShowCreateRoom(false);
                }}
                style={{ ...S.input, fontSize: 12, marginBottom: 4 }}
                autoFocus
              />
              <button
                onClick={() => {
                  if (newRoomName.trim()) {
                    conn?.reducers.createRoom({ name: newRoomName });
                    setNewRoomName('');
                    setShowCreateRoom(false);
                  }
                }}
                style={{ ...S.btn, width: '100%', background: '#4cf490', color: '#0d0d0e', fontWeight: 600 }}
              >
                Create
              </button>
            </div>
          )}

          {rooms.length === 0 && (
            <div style={{ padding: '12px 16px', color: '#6f7987', fontSize: 12 }}>
              Create a room to get started
            </div>
          )}

          {rooms.map(r => {
            const mem = myHex ? roomMembers.find(m => m.roomId === r.id && m.userId.toHexString() === myHex) : null;
            const isMem = mem && !mem.isBanned;
            const unread = isMem ? getUnreadCount(r.id) : 0;
            const sel = selectedRoomId === r.id;
            return (
              <div
                key={r.id.toString()}
                style={{ padding: '5px 16px', display: 'flex', alignItems: 'center', justifyContent: 'space-between', background: sel ? '#202126' : 'transparent', borderRadius: 4, cursor: 'pointer' }}
              >
                <span
                  onClick={() => {
                    if (!isMem) conn?.reducers.joinRoom({ roomId: r.id });
                    setSelectedRoomId(r.id);
                    setShowMembers(false);
                    setShowSchedulePanel(false);
                  }}
                  style={{ flex: 1, fontSize: 13, color: sel ? '#4cf490' : '#e6e9f0' }}
                >
                  # {r.name}
                </span>
                {isMem ? (
                  unread > 0 ? (
                    <span style={{ background: '#4cf490', color: '#0d0d0e', borderRadius: 10, padding: '1px 6px', fontSize: 11, fontWeight: 700 }}>{unread}</span>
                  ) : null
                ) : (
                  <button
                    onClick={() => { conn?.reducers.joinRoom({ roomId: r.id }); setSelectedRoomId(r.id); }}
                    style={{ ...S.btn, padding: '1px 6px', fontSize: 11, borderColor: '#4cf490', color: '#4cf490' }}
                  >
                    Join
                  </button>
                )}
              </div>
            );
          })}
        </div>

        {/* Online users list */}
        <div style={{ borderTop: '1px solid #202126', maxHeight: 180, overflow: 'auto', padding: '8px 0' }}>
          <div style={{ padding: '0 16px 4px' }}>
            <span style={S.sectionLabel}>Online Users</span>
          </div>
          {users.filter(u => u.online && u.status !== 'invisible').map(u => (
            <div key={u.identity.toHexString()} style={{ padding: '3px 16px', display: 'flex', alignItems: 'center', gap: 6 }}>
              <span style={{ width: 6, height: 6, borderRadius: '50%', background: STATUS_COLOR[u.status] ?? '#6f7987', display: 'inline-block', flexShrink: 0 }} />
              <span style={{ fontSize: 12 }}>{u.name}</span>
            </div>
          ))}
        </div>
      </div>

      {/* ── Main area ── */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
        {selectedRoomId ? (
          <>
            {/* Room header */}
            <div style={{ padding: '10px 20px', borderBottom: '1px solid #202126', background: '#141416', display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexShrink: 0 }}>
              <span style={{ fontWeight: 700, fontSize: 15 }}>
                # {rooms.find(r => r.id === selectedRoomId)?.name}
              </span>
              <div style={{ display: 'flex', gap: 8 }}>
                <button
                  onClick={() => { setShowSchedulePanel(v => !v); setShowMembers(false); }}
                  aria-label="schedule messages"
                  title="schedule"
                  style={{ ...S.btn, color: showSchedulePanel ? '#4cf490' : '#6f7987' }}
                >
                  Schedule
                </button>
                <button
                  onClick={() => { setShowMembers(v => !v); setShowSchedulePanel(false); }}
                  style={{ ...S.btn, color: showMembers ? '#4cf490' : '#6f7987' }}
                >
                  Members
                </button>
                {myMembership && !myMembership.isBanned && (
                  <button
                    onClick={() => {
                      leavingRef.current = selectedRoomId;
                      conn?.reducers.leaveRoom({ roomId: selectedRoomId });
                      setSelectedRoomId(null);
                    }}
                    style={{ ...S.btn, color: '#ff4c4c', borderColor: '#ff4c4c44' }}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            {/* Kicked notice */}
            {kickedNotice && (
              <div style={{ background: '#ff4c4c22', border: '1px solid #ff4c4c', color: '#ff4c4c', padding: '8px 20px', fontSize: 13 }}>
                You were kicked from this room.
              </div>
            )}

            {/* Content row */}
            <div style={{ flex: 1, display: 'flex', overflow: 'hidden', position: 'relative' }}>
              {/* Messages */}
              <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px' }}>
                {roomMessages.length === 0 && (
                  <div style={{ textAlign: 'center', color: '#6f7987', marginTop: 60 }}>
                    No messages yet — say something!
                  </div>
                )}

                {roomMessages.map((msg, idx) => {
                  // Skip server-deleted ephemeral (client-side expiry guard)
                  if (msg.isEphemeral && msg.expiresAtMicros > 0n && microsToMs(msg.expiresAtMicros) <= now) {
                    return null;
                  }

                  const sender = users.find(u => u.identity.toHexString() === msg.sender.toHexString());
                  const isMe = msg.sender.toHexString() === myHex;
                  const prev = idx > 0 ? roomMessages[idx - 1] : null;
                  const grouped =
                    !!prev &&
                    prev.sender.toHexString() === msg.sender.toHexString() &&
                    msg.sentAtMicros - prev.sentAtMicros < 120_000_000n;

                  // Reactions grouped by emoji
                  const msgReactions = reactions.filter(r => r.messageId === msg.id);
                  const reactionMap: Record<string, { count: number; names: string[]; mine: boolean }> = {};
                  for (const r of msgReactions) {
                    if (!reactionMap[r.emoji]) reactionMap[r.emoji] = { count: 0, names: [], mine: false };
                    reactionMap[r.emoji].count++;
                    const ru = users.find(u => u.identity.toHexString() === r.userId.toHexString());
                    if (ru) reactionMap[r.emoji].names.push(ru.name);
                    if (r.userId.toHexString() === myHex) reactionMap[r.emoji].mine = true;
                  }

                  // Read receipts (users who have seen up to this message)
                  const seenBy = lastReads
                    .filter(lr => lr.roomId === selectedRoomId && lr.lastMessageId >= msg.id && lr.userId.toHexString() !== msg.sender.toHexString())
                    .map(lr => users.find(u => u.identity.toHexString() === lr.userId.toHexString())?.name)
                    .filter((n): n is string => !!n);

                  const expiresInMs = msg.isEphemeral && msg.expiresAtMicros > 0n ? microsToMs(msg.expiresAtMicros) - now : null;
                  const expiresInSecs = expiresInMs !== null ? Math.max(0, Math.ceil(expiresInMs / 1000)) : null;

                  return (
                    <div
                      key={msg.id.toString()}
                      style={{ marginBottom: grouped ? 1 : 10 }}
                      onMouseEnter={() => setHoveredMsgId(msg.id)}
                      onMouseLeave={() => { setHoveredMsgId(null); setReactPickerMsgId(null); }}
                    >
                      {/* Header row (only for first in group) */}
                      {!grouped && (
                        <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginBottom: 1 }}>
                          <span style={{ fontWeight: 700, fontSize: 13, color: '#a880ff' }}>{sender?.name ?? 'Unknown'}</span>
                          <span style={{ fontSize: 11, color: '#6f7987' }}>{formatTime(msg.sentAtMicros)}</span>
                          {msg.isEdited && (
                            <span
                              style={{ fontSize: 11, color: '#6f7987', cursor: 'pointer', textDecoration: 'underline dotted' }}
                              onClick={() => setHistoryMsgId(historyMsgId === msg.id ? null : msg.id)}
                            >
                              (edited)
                            </span>
                          )}
                          {msg.isEphemeral && (
                            <span style={{ fontSize: 11, color: '#fbdc8e' }}>
                              {expiresInSecs !== null ? `⏱ expires in ${expiresInSecs}s` : '⏱ ephemeral'}
                            </span>
                          )}
                        </div>
                      )}

                      {/* Message body */}
                      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 6 }}>
                        {editingMsgId === msg.id ? (
                          <div style={{ flex: 1 }}>
                            <input
                              type="text"
                              value={editText}
                              onChange={e => setEditText(e.target.value)}
                              onKeyDown={e => {
                                if (e.key === 'Enter' && editText.trim()) {
                                  conn?.reducers.editMessage({ messageId: msg.id, newText: editText });
                                  setEditingMsgId(null);
                                }
                                if (e.key === 'Escape') setEditingMsgId(null);
                              }}
                              style={{ ...S.input, width: '100%', fontSize: 13 }}
                              autoFocus
                            />
                            <div style={{ marginTop: 4, display: 'flex', gap: 6 }}>
                              <button
                                onClick={() => {
                                  if (editText.trim()) {
                                    conn?.reducers.editMessage({ messageId: msg.id, newText: editText });
                                    setEditingMsgId(null);
                                  }
                                }}
                                style={{ ...S.btn, background: '#4cf490', color: '#0d0d0e', fontWeight: 600 }}
                              >
                                Save
                              </button>
                              <button onClick={() => setEditingMsgId(null)} style={S.btn}>Cancel</button>
                            </div>
                          </div>
                        ) : (
                          <>
                            <span style={{ flex: 1, fontSize: 14, color: msg.isEphemeral ? '#fbdc8e' : '#e6e9f0', lineHeight: 1.5 }}>
                              {grouped && msg.isEdited && (
                                <span
                                  style={{ fontSize: 11, color: '#6f7987', cursor: 'pointer', marginRight: 4 }}
                                  onClick={() => setHistoryMsgId(historyMsgId === msg.id ? null : msg.id)}
                                >
                                  (edited)
                                </span>
                              )}
                              {grouped && msg.isEphemeral && expiresInSecs !== null && (
                                <span style={{ fontSize: 11, color: '#fbdc8e', marginRight: 4 }}>
                                  ⏱{expiresInSecs}s
                                </span>
                              )}
                              {msg.text}
                            </span>

                            {/* Hover action buttons */}
                            {hoveredMsgId === msg.id && (
                              <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
                                <button
                                  aria-label="react to message"
                                  onClick={() => setReactPickerMsgId(reactPickerMsgId === msg.id ? null : msg.id)}
                                  style={{ ...S.actionBtn }}
                                >
                                  😊
                                </button>
                                {isMe && (
                                  <button
                                    onClick={() => { setEditingMsgId(msg.id); setEditText(msg.text); }}
                                    style={{ ...S.actionBtn }}
                                  >
                                    Edit
                                  </button>
                                )}
                              </div>
                            )}
                          </>
                        )}
                      </div>

                      {/* Emoji picker */}
                      {reactPickerMsgId === msg.id && (
                        <div style={{ marginTop: 4, display: 'flex', gap: 4 }}>
                          {EMOJIS.map(emoji => (
                            <button
                              key={emoji}
                              onClick={() => { conn?.reducers.toggleReaction({ messageId: msg.id, emoji }); setReactPickerMsgId(null); }}
                              style={{ padding: '2px 7px', borderRadius: 6, border: '1px solid #202126', background: '#141416', cursor: 'pointer', fontSize: 15 }}
                            >
                              {emoji}
                            </button>
                          ))}
                        </div>
                      )}

                      {/* Reactions */}
                      {Object.keys(reactionMap).length > 0 && (
                        <div style={{ marginTop: 3, display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                          {Object.entries(reactionMap).map(([emoji, data]) => (
                            <button
                              key={emoji}
                              title={data.names.join(', ')}
                              onClick={() => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                              style={{
                                padding: '1px 7px',
                                borderRadius: 10,
                                border: `1px solid ${data.mine ? '#4cf490' : '#202126'}`,
                                background: data.mine ? '#4cf49018' : '#141416',
                                cursor: 'pointer',
                                fontSize: 12,
                                color: '#e6e9f0',
                              }}
                            >
                              {emoji} {data.count}
                            </button>
                          ))}
                        </div>
                      )}

                      {/* Read receipts */}
                      {seenBy.length > 0 && (
                        <div style={{ fontSize: 10, color: '#6f7987', marginTop: 1 }}>
                          Seen by {seenBy.join(', ')}
                        </div>
                      )}

                      {/* Edit history inline */}
                      {historyMsgId === msg.id && (
                        <div style={{ marginTop: 6, padding: 10, background: '#0d0d0e', border: '1px solid #202126', borderRadius: 8 }}>
                          <div style={{ fontSize: 12, fontWeight: 600, color: '#6f7987', marginBottom: 6 }}>Edit History</div>
                          {editHistories
                            .filter(h => h.messageId === msg.id)
                            .sort((a, b) => (a.editedAtMicros < b.editedAtMicros ? -1 : 1))
                            .map(h => (
                              <div key={h.id.toString()} style={{ marginBottom: 4 }}>
                                <span style={{ fontSize: 13, color: '#e6e9f0' }}>{h.text}</span>
                                <span style={{ fontSize: 10, color: '#6f7987', marginLeft: 8 }}>
                                  {new Date(microsToMs(h.editedAtMicros)).toLocaleString()}
                                </span>
                              </div>
                            ))}
                          {editHistories.filter(h => h.messageId === msg.id).length === 0 && (
                            <div style={{ fontSize: 12, color: '#6f7987' }}>No history yet</div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                {/* Typing indicator */}
                {typingUsers.length > 0 && (
                  <div style={{ color: '#6f7987', fontSize: 12, fontStyle: 'italic', padding: '4px 0' }}>
                    {typingUsers.length === 1
                      ? `${typingUsers[0]} is typing…`
                      : `${typingUsers.slice(0, -1).join(', ')} and ${typingUsers[typingUsers.length - 1]} are typing…`}
                  </div>
                )}

                <div ref={messagesEndRef} />
              </div>

              {/* Members panel */}
              {showMembers && (
                <div style={S.panel}>
                  <div style={{ fontWeight: 700, fontSize: 14, marginBottom: 12 }}>Members</div>
                  {roomMembers
                    .filter(m => m.roomId === selectedRoomId && !m.isBanned)
                    .map(m => {
                      const u = users.find(u2 => u2.identity.toHexString() === m.userId.toHexString());
                      const isMyself = m.userId.toHexString() === myHex;
                      return (
                        <div key={m.id.toString()} style={{ marginBottom: 10 }}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 6, justifyContent: 'space-between' }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                              <span style={{ width: 7, height: 7, borderRadius: '50%', background: u ? STATUS_COLOR[u.status] ?? '#6f7987' : '#6f7987', display: 'inline-block' }} />
                              <span style={{ fontSize: 13 }}>{u?.name ?? 'Unknown'}</span>
                              {m.isAdmin && (
                                <span style={{ fontSize: 10, color: '#4cf490', background: '#4cf49018', padding: '1px 5px', borderRadius: 4 }}>
                                  Admin
                                </span>
                              )}
                            </div>
                            {isAdmin && !isMyself && !m.isAdmin && (
                              <div style={{ display: 'flex', gap: 4 }}>
                                <button
                                  onClick={() => conn?.reducers.kickUser({ roomId: selectedRoomId, userId: m.userId })}
                                  style={{ ...S.btn, color: '#ff4c4c', borderColor: '#ff4c4c55', fontSize: 11 }}
                                >
                                  Kick
                                </button>
                                <button
                                  onClick={() => conn?.reducers.promoteUser({ roomId: selectedRoomId, userId: m.userId })}
                                  style={{ ...S.btn, color: '#4cf490', borderColor: '#4cf49055', fontSize: 11 }}
                                >
                                  Promote
                                </button>
                              </div>
                            )}
                          </div>
                          {u && !u.online && (
                            <div style={{ fontSize: 11, color: '#6f7987', marginLeft: 13 }}>
                              Last active {formatRelative(u.lastActiveMicros)}
                            </div>
                          )}
                        </div>
                      );
                    })}
                </div>
              )}

              {/* Schedule panel */}
              {showSchedulePanel && (
                <div style={S.panel}>
                  <div style={{ fontWeight: 700, fontSize: 14, marginBottom: 12 }}>Schedule Message</div>
                  <input
                    type="datetime-local"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                    style={{ ...S.input, marginBottom: 8, fontSize: 12 }}
                  />
                  <textarea
                    value={scheduleText}
                    onChange={e => setScheduleText(e.target.value)}
                    placeholder="Message to schedule…"
                    rows={3}
                    style={{ ...S.input, resize: 'none', marginBottom: 8, fontSize: 12 }}
                  />
                  <button
                    onClick={() => {
                      if (scheduleText.trim() && scheduleTime && selectedRoomId) {
                        const micros = BigInt(new Date(scheduleTime).getTime()) * 1000n;
                        conn?.reducers.scheduleMessage({ roomId: selectedRoomId, text: scheduleText, sendAtMicros: micros });
                        setScheduleText('');
                        setScheduleTime('');
                      }
                    }}
                    style={{ ...S.btn, width: '100%', background: '#4cf490', color: '#0d0d0e', fontWeight: 700, marginBottom: 16 }}
                  >
                    Schedule
                  </button>

                  <div style={{ fontSize: 12, fontWeight: 600, color: '#6f7987', marginBottom: 8, textTransform: 'uppercase', letterSpacing: 0.5 }}>
                    Scheduled / Pending
                  </div>
                  {scheduledDeliveries
                    .filter(s => s.sender.toHexString() === myHex && s.roomId === selectedRoomId)
                    .length === 0 && (
                    <div style={{ fontSize: 12, color: '#6f7987' }}>No pending messages</div>
                  )}
                  {scheduledDeliveries
                    .filter(s => s.sender.toHexString() === myHex && s.roomId === selectedRoomId)
                    .map(s => (
                      <div key={s.scheduledId.toString()} style={{ marginBottom: 8, padding: 8, background: '#0d0d0e', border: '1px solid #202126', borderRadius: 6 }}>
                        <div style={{ fontSize: 12, color: '#e6e9f0', marginBottom: 4 }}>{s.text}</div>
                        <button
                          onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: s.scheduledId })}
                          style={{ ...S.btn, color: '#ff4c4c', borderColor: '#ff4c4c55', fontSize: 11 }}
                        >
                          Cancel
                        </button>
                      </div>
                    ))}
                </div>
              )}
            </div>

            {/* Input bar */}
            <div style={{ padding: '10px 20px', borderTop: '1px solid #202126', background: '#141416', flexShrink: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
                <label style={{ fontSize: 12, color: '#6f7987', whiteSpace: 'nowrap' }}>Disappear after:</label>
                <select
                  value={ephemeralSecs}
                  onChange={e => setEphemeralSecs(Number(e.target.value))}
                  style={{ ...S.input, padding: '3px 8px', fontSize: 12, width: 'auto' }}
                >
                  <option value={0}>Never (normal)</option>
                  <option value={30}>30 seconds (expire)</option>
                  <option value={60}>1 minute (expire)</option>
                  <option value={300}>5 minutes (expire)</option>
                </select>
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <input
                  type="text"
                  placeholder={`Message #${rooms.find(r => r.id === selectedRoomId)?.name ?? ''}…`}
                  value={messageText}
                  onChange={e => { setMessageText(e.target.value); if (e.target.value) fireTyping(); }}
                  onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
                  style={{ ...S.input, flex: 1, padding: '10px 14px', fontSize: 14 }}
                />
                <button
                  onClick={handleSend}
                  disabled={!messageText.trim()}
                  style={{ padding: '10px 20px', borderRadius: 8, border: 'none', background: messageText.trim() ? '#4cf490' : '#202126', color: messageText.trim() ? '#0d0d0e' : '#6f7987', fontWeight: 700, cursor: messageText.trim() ? 'pointer' : 'default', fontSize: 14 }}
                >
                  Send
                </button>
              </div>
            </div>
          </>
        ) : (
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', color: '#6f7987', gap: 8 }}>
            {kickedNotice && (
              <div style={{ color: '#ff4c4c', fontSize: 14, marginBottom: 8 }}>You were kicked from that room.</div>
            )}
            <div style={{ fontSize: 36 }}>💬</div>
            <div style={{ fontSize: 16 }}>Select a room to start chatting</div>
            <div style={{ fontSize: 13 }}>Create a room to get started</div>
          </div>
        )}
      </div>

      {/* ── Right panel: all users with rich presence ── */}
      <div style={{ width: 200, background: '#141416', borderLeft: '1px solid #202126', padding: '12px 0', overflow: 'auto', flexShrink: 0 }}>
        <div style={{ padding: '0 14px 8px' }}>
          <span style={S.sectionLabel}>All Users</span>
        </div>
        {users.map(u => (
          <div key={u.identity.toHexString()} style={{ padding: '5px 14px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
              <span style={{ width: 8, height: 8, borderRadius: '50%', background: STATUS_COLOR[u.status] ?? '#6f7987', display: 'inline-block', flexShrink: 0 }} />
              <span style={{ fontSize: 13, fontWeight: u.identity.toHexString() === myHex ? 700 : 400 }}>{u.name}</span>
            </div>
            <div style={{ fontSize: 11, color: '#6f7987', marginLeft: 15 }}>
              {u.status === 'online' ? 'Online'
                : u.status === 'away' ? 'Away'
                : u.status === 'dnd' ? 'Do Not Disturb'
                : 'Invisible'}
              {!u.online && ` · Last active ${formatRelative(u.lastActiveMicros)}`}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Shared styles ────────────────────────────────────────────────────────────

const S = {
  center: {
    display: 'flex',
    height: '100vh',
    alignItems: 'center',
    justifyContent: 'center',
    background: '#0d0d0e',
    color: '#e6e9f0',
  } as const,

  card: {
    background: '#141416',
    border: '1px solid #202126',
    borderRadius: 12,
    padding: 28,
    width: 320,
  } as const,

  sidebar: {
    width: 220,
    background: '#141416',
    borderRight: '1px solid #202126',
    display: 'flex',
    flexDirection: 'column',
    flexShrink: 0,
    overflow: 'hidden',
  } as const,

  panel: {
    width: 260,
    background: '#141416',
    borderLeft: '1px solid #202126',
    padding: 16,
    overflow: 'auto',
    flexShrink: 0,
  } as const,

  input: {
    width: '100%',
    padding: '7px 10px',
    borderRadius: 7,
    border: '1px solid #202126',
    background: '#0d0d0e',
    color: '#e6e9f0',
    fontSize: 13,
    fontFamily: 'inherit',
  } as const,

  btn: {
    padding: '4px 10px',
    borderRadius: 6,
    border: '1px solid #202126',
    background: 'transparent',
    color: '#6f7987',
    cursor: 'pointer',
    fontSize: 12,
    fontFamily: 'inherit',
  } as const,

  iconBtn: {
    background: 'none',
    border: 'none',
    color: '#4cf490',
    cursor: 'pointer',
    fontSize: 20,
    padding: '0 4px',
    lineHeight: 1,
    fontFamily: 'inherit',
  } as const,

  actionBtn: {
    padding: '2px 7px',
    borderRadius: 4,
    border: '1px solid #202126',
    background: '#141416',
    color: '#6f7987',
    cursor: 'pointer',
    fontSize: 12,
    fontFamily: 'inherit',
  } as const,

  sectionLabel: {
    fontSize: 10,
    fontWeight: 700,
    color: '#6f7987',
    textTransform: 'uppercase' as const,
    letterSpacing: 0.8,
  } as const,
};
