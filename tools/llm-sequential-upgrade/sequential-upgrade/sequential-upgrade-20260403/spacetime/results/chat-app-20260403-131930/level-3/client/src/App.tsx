import { useEffect, useRef, useState } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Room, Message, User, TypingIndicator, ReadReceipt, ScheduledMessage } from './module_bindings/types';

// ---- Timestamp helper ----
function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  const ms = Number(ts.microsSinceUnixEpoch / 1000n);
  const d = new Date(ms);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ---- Main App ----
export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [activeRoomId, setActiveRoomId] = useState<bigint | null>(null);
  const [showCreateRoom, setShowCreateRoom] = useState(false);

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe when connected
  useEffect(() => {
    if (!conn || !isActive) return;
    const sub = conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM scheduled_message',
      ]);
    return () => { sub.unsubscribe(); };
  }, [conn, isActive]);

  // Reactive data
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [members] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  const myHex = myIdentity?.toHexString();

  const me = myHex ? users.find((u: User) => u.identity.toHexString() === myHex) : null;
  const isRegistered = !!me;

  // --- Loading state ---
  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        Connecting to SpacetimeDB…
      </div>
    );
  }

  // --- Register ---
  if (!isRegistered) {
    return (
      <RegisterScreen
        conn={conn}
      />
    );
  }

  // Compute my rooms
  const myMemberships = members.filter((m) => m.userIdentity.toHexString() === myHex);
  const myRoomIds = new Set(myMemberships.map((m) => m.roomId));
  const myRooms = rooms.filter((r: Room) => myRoomIds.has(r.id));
  const otherRooms = rooms.filter((r: Room) => !myRoomIds.has(r.id));

  const activeRoom = activeRoomId ? rooms.find((r: Room) => r.id === activeRoomId) : null;
  const roomMessages = activeRoomId
    ? messages.filter((m: Message) => m.roomId === activeRoomId)
        .sort((a: Message, b: Message) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    : [];

  const onlineUsers = users.filter((u: User) => u.online);

  // Unread counts per room
  const unreadCounts: Map<bigint, number> = new Map();
  for (const room of myRooms) {
    const receipt = readReceipts.find(
      (r: ReadReceipt) => r.roomId === room.id && r.userIdentity.toHexString() === myHex
    );
    const lastReadId = receipt ? receipt.lastReadMessageId : 0n;
    const count = messages.filter(
      (m: Message) => m.roomId === room.id && m.id > lastReadId && m.sender.toHexString() !== myHex
    ).length;
    unreadCounts.set(room.id, count);
  }

  // My scheduled messages for the active room
  const myRoomScheduled = activeRoomId
    ? scheduledMessages.filter(
        (sm: ScheduledMessage) => sm.roomId === activeRoomId && sm.sender.toHexString() === myHex
      )
    : [];

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="sidebar-title">SpacetimeDB Chat</div>
        </div>
        <div className="sidebar-user">
          <span className="status-dot online" />
          <span style={{ fontWeight: 600 }}>{me.name}</span>
        </div>
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
          </div>
          {myRooms.length === 0 && otherRooms.length === 0 && (
            <div style={{ padding: '8px 16px', color: 'var(--text-muted)', fontSize: 12 }}>
              No rooms yet. Create one!
            </div>
          )}
          {myRooms.map((r: Room) => (
            <div
              key={String(r.id)}
              className={`room-item ${activeRoomId === r.id ? 'active' : ''}`}
              onClick={() => setActiveRoomId(r.id)}
            >
              <span className="room-name"># {r.name}</span>
              {(unreadCounts.get(r.id) ?? 0) > 0 && (
                <span className="unread-badge">{unreadCounts.get(r.id)}</span>
              )}
            </div>
          ))}
          {otherRooms.length > 0 && (
            <>
              <div className="sidebar-section-header" style={{ marginTop: 8 }}>
                <span>Other Rooms</span>
              </div>
              {otherRooms.map((r: Room) => (
                <div
                  key={String(r.id)}
                  className="room-item"
                  onClick={() => {
                    conn?.reducers.joinRoom({ roomId: r.id });
                    setActiveRoomId(r.id);
                  }}
                >
                  <span className="room-name"># {r.name}</span>
                  <button className="secondary" style={{ fontSize: 11, padding: '2px 8px' }}
                    onClick={(e) => {
                      e.stopPropagation();
                      conn?.reducers.joinRoom({ roomId: r.id });
                      setActiveRoomId(r.id);
                    }}>Join</button>
                </div>
              ))}
            </>
          )}
          <div className="sidebar-section-header" style={{ marginTop: 8 }}>
            <span>Online ({onlineUsers.length})</span>
          </div>
          {onlineUsers.map((u: User) => (
            <div key={u.identity.toHexString()} className="online-user-item">
              <span className="status-dot online" />
              <span>{u.name}{u.identity.toHexString() === myHex ? ' (you)' : ''}</span>
            </div>
          ))}
        </div>
      </aside>

      {/* Main */}
      <main className="main">
        {activeRoom ? (
          <RoomView
            room={activeRoom}
            messages={roomMessages}
            users={users}
            myHex={myHex!}
            typingIndicators={typingIndicators}
            readReceipts={readReceipts}
            scheduledMessages={myRoomScheduled}
            conn={conn}
            onLeave={() => {
              conn?.reducers.leaveRoom({ roomId: activeRoom.id });
              setActiveRoomId(null);
            }}
          />
        ) : (
          <div className="empty-state">
            <h3>Welcome, {me.name}!</h3>
            <p>Select a room from the sidebar to start chatting,<br/>or create a new room.</p>
            <button className="primary" onClick={() => setShowCreateRoom(true)}>+ Create Room</button>
          </div>
        )}
      </main>

      {showCreateRoom && (
        <CreateRoomModal
          conn={conn}
          onClose={() => setShowCreateRoom(false)}
          rooms={rooms}
        />
      )}

    </div>
  );
}

// ---- Register Screen ----
function RegisterScreen({ conn }: { conn: DbConnection | null }) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  const handleSubmit = () => {
    const trimmed = name.trim();
    if (!trimmed) { setError('Please enter a name'); return; }
    if (trimmed.length > 32) { setError('Name too long (max 32)'); return; }
    setError('');
    conn?.reducers.register({ name: trimmed });
  };

  return (
    <div className="login-screen">
      <div className="login-card">
        <div className="login-title">SpacetimeDB Chat</div>
        <div className="login-subtitle">Enter a display name to get started</div>
        <div className="form-group">
          <input
            type="text"
            placeholder="Enter your name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleSubmit()}
            autoFocus
            maxLength={32}
          />
          {error && <div className="error-msg">{error}</div>}
        </div>
        <button className="primary" onClick={handleSubmit} type="submit">Join Chat</button>
      </div>
    </div>
  );
}

// ---- Create Room Modal ----
function CreateRoomModal({
  conn, onClose, rooms
}: {
  conn: DbConnection | null;
  onClose: () => void;
  rooms: ReadonlyArray<Room>;
}) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  const handleCreate = () => {
    const trimmed = name.trim();
    if (!trimmed) { setError('Room name required'); return; }
    if (trimmed.length > 64) { setError('Name too long (max 64)'); return; }
    const exists = rooms.some((r: Room) => r.name.toLowerCase() === trimmed.toLowerCase());
    if (exists) { setError('Room already exists'); return; }
    setError('');
    conn?.reducers.createRoom({ name: trimmed });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Create a New Room</div>
        <div className="form-group">
          <input
            type="text"
            placeholder="Room name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') handleCreate(); if (e.key === 'Escape') onClose(); }}
            autoFocus
            maxLength={64}
          />
          {error && <div className="error-msg">{error}</div>}
        </div>
        <div className="modal-actions">
          <button className="secondary" onClick={onClose}>Cancel</button>
          <button className="primary" onClick={handleCreate}>Create Room</button>
        </div>
      </div>
    </div>
  );
}

// ---- Schedule Message Modal ----
function ScheduleMessageModal({
  conn, roomId, onClose
}: {
  conn: DbConnection | null;
  roomId: bigint;
  onClose: () => void;
}) {
  const [text, setText] = useState('');
  const [dateValue, setDateValue] = useState('');
  const [timeValue, setTimeValue] = useState('');
  const [error, setError] = useState('');

  // Default to 5 minutes from now
  useEffect(() => {
    const now = new Date(Date.now() + 5 * 60 * 1000);
    const dateStr = now.toISOString().slice(0, 10);
    const hours = String(now.getHours()).padStart(2, '0');
    const mins = String(now.getMinutes()).padStart(2, '0');
    setDateValue(dateStr);
    setTimeValue(`${hours}:${mins}`);
  }, []);

  const handleSchedule = () => {
    const trimmed = text.trim();
    if (!trimmed) { setError('Message cannot be empty'); return; }
    if (!dateValue || !timeValue) { setError('Please set a date and time'); return; }
    const sendAt = new Date(`${dateValue}T${timeValue}:00`);
    if (isNaN(sendAt.getTime())) { setError('Invalid date/time'); return; }
    if (sendAt.getTime() <= Date.now()) { setError('Scheduled time must be in the future'); return; }
    const sendAtMicros = BigInt(sendAt.getTime()) * 1000n;
    setError('');
    conn?.reducers.scheduleMessage({ roomId, text: trimmed, sendAtMicros });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-title">Schedule a Message</div>
        <div className="form-group">
          <textarea
            placeholder="Type your message..."
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={3}
            maxLength={2000}
            style={{ width: '100%', resize: 'vertical', background: 'var(--bg)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: 6, padding: '8px 12px', fontFamily: 'inherit', fontSize: 14 }}
            autoFocus
          />
        </div>
        <div className="form-group" style={{ display: 'flex', gap: 8 }}>
          <input
            type="date"
            value={dateValue}
            onChange={(e) => setDateValue(e.target.value)}
            style={{ flex: 1 }}
          />
          <input
            type="time"
            value={timeValue}
            onChange={(e) => setTimeValue(e.target.value)}
            style={{ flex: 1 }}
          />
        </div>
        {error && <div className="error-msg">{error}</div>}
        <div className="modal-actions">
          <button className="secondary" onClick={onClose}>Cancel</button>
          <button className="primary" onClick={handleSchedule}>Schedule</button>
        </div>
      </div>
    </div>
  );
}

// ---- Room View ----
function RoomView({
  room, messages, users, myHex, typingIndicators, readReceipts, scheduledMessages, conn, onLeave
}: {
  room: Room;
  messages: ReadonlyArray<Message>;
  users: ReadonlyArray<User>;
  myHex: string;
  typingIndicators: ReadonlyArray<TypingIndicator>;
  readReceipts: ReadonlyArray<ReadReceipt>;
  scheduledMessages: ReadonlyArray<ScheduledMessage>;
  conn: DbConnection | null;
  onLeave: () => void;
}) {
  const [text, setText] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [, setTick] = useState(0);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (isAtBottom && messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, isAtBottom]);

  // Mark messages as read when room is active
  useEffect(() => {
    if (messages.length === 0) return;
    const lastMsg = messages[messages.length - 1];
    conn?.reducers.markRead({ roomId: room.id, lastReadMessageId: lastMsg.id });
  }, [messages, room.id, conn]);

  const handleScroll = () => {
    const container = messagesContainerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    setIsAtBottom(scrollHeight - scrollTop - clientHeight < 50);
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    setIsAtBottom(true);
  };

  const handleTyping = () => {
    if (!isTypingRef.current) {
      isTypingRef.current = true;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: true });
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      isTypingRef.current = false;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
    }, 4000);
  };

  // Cleanup typing on unmount / room change
  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      if (isTypingRef.current) {
        conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
        isTypingRef.current = false;
      }
    };
  }, [room.id, conn]);

  // Tick every second to update ephemeral countdown displays
  useEffect(() => {
    const hasEphemeral = messages.some((m: Message) => m.expiresAtMicros > 0n);
    if (!hasEphemeral) return;
    const timer = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(timer);
  }, [messages]);

  const handleSend = () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText('');
    // Clear typing
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    if (isTypingRef.current) {
      isTypingRef.current = false;
      conn?.reducers.setTyping({ roomId: room.id, isTyping: false });
    }
    if (isEphemeral) {
      conn?.reducers.sendEphemeralMessage({ roomId: room.id, text: trimmed, durationSeconds: ephemeralDuration });
    } else {
      conn?.reducers.sendMessage({ roomId: room.id, text: trimmed });
    }
  };

  // Format countdown for ephemeral messages
  const formatCountdown = (expiresAtMicros: bigint): string => {
    const nowMicros = BigInt(Date.now()) * 1000n;
    const remaining = expiresAtMicros - nowMicros;
    if (remaining <= 0n) return 'Expiring…';
    const secs = Number(remaining / 1_000_000n);
    if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
    if (secs >= 60) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    return `${secs}s`;
  };

  // Typing indicator text (other users typing in this room)
  const roomTyping = typingIndicators.filter(
    (ti: TypingIndicator) => ti.roomId === room.id && ti.userIdentity.toHexString() !== myHex
  );
  const typingNames = roomTyping.map((ti: TypingIndicator) => {
    const u = users.find((u: User) => u.identity.toHexString() === ti.userIdentity.toHexString());
    return u?.name ?? 'Someone';
  });

  let typingText = '';
  if (typingNames.length === 1) typingText = `${typingNames[0]} is typing...`;
  else if (typingNames.length === 2) typingText = `${typingNames[0]} and ${typingNames[1]} are typing...`;
  else if (typingNames.length > 2) typingText = 'Multiple users are typing...';

  // Group messages by sender
  const groupedMessages: { sender: string; senderName: string; isMe: boolean; items: Message[] }[] = [];
  for (const msg of messages) {
    const senderHex = msg.sender.toHexString();
    const isMe = senderHex === myHex;
    const senderUser = users.find((u: User) => u.identity.toHexString() === senderHex);
    const senderName = senderUser?.name ?? 'Unknown';
    const last = groupedMessages[groupedMessages.length - 1];
    if (last && last.sender === senderHex) {
      last.items.push(msg);
    } else {
      groupedMessages.push({ sender: senderHex, senderName, isMe, items: [msg] });
    }
  }

  // Read receipts for a message: users who have read up to this message ID (excluding the message sender)
  const getReadBy = (msgId: bigint, senderHex: string): string[] => {
    return readReceipts
      .filter((r: ReadReceipt) => r.roomId === room.id && r.lastReadMessageId >= msgId && r.userIdentity.toHexString() !== myHex && r.userIdentity.toHexString() !== senderHex)
      .map((r: ReadReceipt) => {
        const u = users.find((u: User) => u.identity.toHexString() === r.userIdentity.toHexString());
        return u?.name ?? 'Someone';
      });
  };

  // Format scheduled time from ScheduleAt
  const formatScheduledAt = (sm: ScheduledMessage): string => {
    const sat = sm.scheduledAt as any;
    let micros: bigint | null = null;
    if (sat && sat.tag === 'Time' && sat.value) {
      micros = BigInt(sat.value.__timestamp_micros_since_unix_epoch__ ?? 0n);
    } else if (typeof sat?.__timestamp_micros_since_unix_epoch__ === 'bigint') {
      micros = sat.__timestamp_micros_since_unix_epoch__;
    }
    if (micros === null) return 'unknown time';
    const ms = Number(micros / 1000n);
    return new Date(ms).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  };

  return (
    <>
      <div className="room-header">
        <span className="room-header-name"># {room.name}</span>
        <button className="danger" onClick={onLeave} style={{ fontSize: 12, padding: '4px 10px' }}>Leave</button>
      </div>

      {/* Scheduled messages panel */}
      {scheduledMessages.length > 0 && (
        <div className="scheduled-panel">
          <div className="scheduled-panel-title">Scheduled Messages ({scheduledMessages.length})</div>
          {scheduledMessages.map((sm: ScheduledMessage) => (
            <div key={String(sm.scheduledId)} className="scheduled-item">
              <div className="scheduled-item-text">{sm.text}</div>
              <div className="scheduled-item-meta">
                <span className="scheduled-item-time">Sends at {formatScheduledAt(sm)}</span>
                <button
                  className="danger"
                  style={{ fontSize: 11, padding: '2px 8px' }}
                  onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: sm.scheduledId })}
                >
                  Cancel
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="messages-container">
        <div
          className="messages-area"
          ref={messagesContainerRef}
          onScroll={handleScroll}
        >
          {messages.length === 0 && (
            <div className="empty-state" style={{ flex: 1, paddingTop: 48 }}>
              <p>No messages yet. Say hello!</p>
            </div>
          )}
          {groupedMessages.map((group, gi) => (
            <div key={gi} className="message-group">
              <div className="message-header">
                <span className={`message-sender ${group.isMe ? 'me' : ''}`}>{group.senderName}</span>
                <span className="message-time">{formatTime(group.items[0].sentAt)}</span>
              </div>
              {group.items.map((msg: Message) => {
                const readBy = getReadBy(msg.id, group.sender);
                const isEphemeralMsg = msg.expiresAtMicros > 0n;
                return (
                  <div key={String(msg.id)} className={`message-row${isEphemeralMsg ? ' ephemeral-message' : ''}`}>
                    <div className="message-text">{msg.text}</div>
                    {isEphemeralMsg && (
                      <div className="ephemeral-indicator" title="This message will disappear">
                        ⏳ Disappears in {formatCountdown(msg.expiresAtMicros)}
                      </div>
                    )}
                    {readBy.length > 0 && (
                      <div className="read-receipt">Seen by {readBy.join(', ')}</div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
          <div ref={messagesEndRef} />
        </div>
        {!isAtBottom && (
          <button className="scroll-btn" onClick={scrollToBottom} title="Scroll to bottom">↓</button>
        )}
      </div>
      <div className="typing-indicator">{typingText}</div>
      <div className="input-bar">
        {isEphemeral && (
          <div className="ephemeral-bar">
            <span style={{ color: 'var(--warning)', fontSize: 12 }}>⏳ Ephemeral:</span>
            <select
              value={ephemeralDuration}
              onChange={(e) => setEphemeralDuration(Number(e.target.value))}
              style={{ background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: 4, padding: '2px 6px', fontSize: 12 }}
            >
              <option value={30}>30 seconds</option>
              <option value={60}>1 minute</option>
              <option value={300}>5 minutes</option>
              <option value={1800}>30 minutes</option>
              <option value={3600}>1 hour</option>
            </select>
          </div>
        )}
        <div className="input-row">
          <input
            type="text"
            placeholder={isEphemeral ? `Ephemeral message (${ephemeralDuration}s)…` : 'Type a message...'}
            value={text}
            onChange={(e) => { setText(e.target.value); handleTyping(); }}
            onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
            maxLength={2000}
            style={isEphemeral ? { borderColor: 'var(--warning)' } : undefined}
          />
          <button className="primary" onClick={handleSend}>Send</button>
          <button
            className={isEphemeral ? 'primary' : 'secondary'}
            onClick={() => setIsEphemeral(!isEphemeral)}
            title={isEphemeral ? 'Disable ephemeral mode' : 'Send disappearing message'}
            style={{ whiteSpace: 'nowrap', background: isEphemeral ? 'var(--warning)' : undefined, color: isEphemeral ? '#000' : undefined }}
          >
            ⏳ Ephemeral
          </button>
          <button
            className="secondary"
            onClick={() => setShowScheduleModal(true)}
            title="Schedule message"
            style={{ whiteSpace: 'nowrap' }}
          >
            ⏰ Schedule
          </button>
        </div>
      </div>

      {showScheduleModal && (
        <ScheduleMessageModal
          conn={conn}
          roomId={room.id}
          onClose={() => setShowScheduleModal(false)}
        />
      )}
    </>
  );
}
