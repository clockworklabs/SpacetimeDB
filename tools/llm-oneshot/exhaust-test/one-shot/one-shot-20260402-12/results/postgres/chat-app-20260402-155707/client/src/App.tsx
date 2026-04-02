import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ---- Types ----

interface User {
  id: number;
  name: string;
  status: string;
  lastActive: string;
  isOnline?: boolean;
}

interface Room {
  id: number;
  name: string;
  createdBy: number;
  unreadCount: number;
}

interface Reaction {
  emoji: string;
  count: number;
  users: string[];
  hasReacted: boolean;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  userName: string;
  content: string;
  createdAt: string;
  expiresAt: string | null;
  scheduledFor: string | null;
  isSent: boolean;
  isDeleted: boolean;
  readBy: { userId: number; userName: string }[];
  reactions: Reaction[];
  _expiresInMs?: number;
}

interface TypingUser {
  userId: number;
  userName: string;
}

// ---- Constants ----

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '🔥'];
const EPHEMERAL_OPTIONS = [
  { label: '1 min', ms: 60_000 },
  { label: '5 min', ms: 300_000 },
  { label: '1 hr', ms: 3_600_000 },
];

function formatTime(dateStr: string) {
  return new Date(dateStr).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatRelative(dateStr: string) {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function EphemeralCountdown({ expiresAt }: { expiresAt: string }) {
  const [remaining, setRemaining] = useState(() => Math.max(0, new Date(expiresAt).getTime() - Date.now()));

  useEffect(() => {
    const timer = setInterval(() => {
      setRemaining(Math.max(0, new Date(expiresAt).getTime() - Date.now()));
    }, 1000);
    return () => clearInterval(timer);
  }, [expiresAt]);

  const secs = Math.ceil(remaining / 1000);
  const mins = Math.floor(secs / 60);
  const display = mins > 0 ? `${mins}m ${secs % 60}s` : `${secs}s`;

  return <span className="ephemeral-badge">⏳ {display}</span>;
}

// ---- App ----

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [loginError, setLoginError] = useState('');

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = nameInput.trim();
    if (!trimmed || trimmed.length > 30) {
      setLoginError('Name must be 1-30 characters');
      return;
    }
    const res = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: trimmed }),
    });
    if (!res.ok) {
      setLoginError('Failed to register. Try a different name.');
      return;
    }
    const user: User = await res.json();
    setCurrentUser(user);
    setLoginError('');
  };

  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>PostgreSQL Chat</h1>
          <p className="login-subtitle">Enter your display name to join</p>
          <form onSubmit={handleLogin} className="login-form">
            <input
              className="login-input"
              type="text"
              placeholder="Display name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              autoFocus
              maxLength={30}
            />
            {loginError && <p className="error-msg">{loginError}</p>}
            <button className="btn-primary" type="submit">Join Chat</button>
          </form>
        </div>
      </div>
    );
  }

  return <ChatApp currentUser={currentUser} setCurrentUser={setCurrentUser} />;
}

// ---- ChatApp ----

function ChatApp({ currentUser, setCurrentUser }: { currentUser: User; setCurrentUser: (u: User) => void }) {
  const [socket, setSocket] = useState<Socket | null>(null);
  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoom, setCurrentRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [typingUsers, setTypingUsers] = useState<TypingUser[]>([]);
  const [newRoomName, setNewRoomName] = useState('');
  const [messageInput, setMessageInput] = useState('');
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [ephemeralMs, setEphemeralMs] = useState<number | null>(null);
  const [scheduledMessages, setScheduledMessages] = useState<Message[]>([]);
  const [showUserList, setShowUserList] = useState(false);
  const [hoveredReaction, setHoveredReaction] = useState<{ msgId: number; emoji: string } | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const lastTypingRef = useRef<number>(0);

  // Connect socket
  useEffect(() => {
    const s = io('/', { transports: ['websocket', 'polling'] });
    s.on('connect', () => {
      s.emit('authenticate', { userId: currentUser.id });
    });
    setSocket(s);
    return () => { s.disconnect(); };
  }, [currentUser.id]);

  // Load rooms and users
  const loadRooms = useCallback(async () => {
    const res = await fetch('/api/rooms');
    const data: Room[] = await res.json();
    // Compute unread for each room
    const enriched = await Promise.all(data.map(async (room) => {
      const msgs = await fetch(`/api/rooms/${room.id}/messages?userId=${currentUser.id}`).then(r => r.json()) as Message[];
      const unread = msgs.filter(m => !m.readBy.some(r => r.userId === currentUser.id)).length;
      return { ...room, unreadCount: unread };
    }));
    setRooms(enriched);
  }, [currentUser.id]);

  const loadUsers = useCallback(async () => {
    const res = await fetch('/api/users');
    const data: User[] = await res.json();
    setOnlineUsers(data);
  }, []);

  useEffect(() => {
    loadRooms();
    loadUsers();
    const interval = setInterval(loadUsers, 30_000);
    return () => clearInterval(interval);
  }, [loadRooms, loadUsers]);

  // Socket events
  useEffect(() => {
    if (!socket) return;

    socket.on('new_message', (msg: Message) => {
      setMessages(prev => {
        if (prev.some(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      // Update unread counts
      setRooms(prev => prev.map(r => {
        if (r.id !== msg.roomId) return r;
        if (currentRoom?.id === msg.roomId) return r; // Currently viewing
        return { ...r, unreadCount: r.unreadCount + 1 };
      }));
    });

    socket.on('message_deleted', ({ messageId }: { messageId: number }) => {
      setMessages(prev => prev.filter(m => m.id !== messageId));
    });

    socket.on('typing_update', (data: { roomId: number; userId: number; userName: string; isTyping: boolean }) => {
      if (data.userId === currentUser.id) return;
      setTypingUsers(prev => {
        if (data.isTyping) {
          if (prev.some(u => u.userId === data.userId)) return prev;
          return [...prev, { userId: data.userId, userName: data.userName }];
        } else {
          return prev.filter(u => u.userId !== data.userId);
        }
      });
    });

    socket.on('messages_read', (data: { roomId: number; userId: number; userName: string; messageIds: number[] }) => {
      if (data.userId === currentUser.id) return;
      setMessages(prev => prev.map(m => {
        if (!data.messageIds.includes(m.id)) return m;
        if (m.readBy.some(r => r.userId === data.userId)) return m;
        return { ...m, readBy: [...m.readBy, { userId: data.userId, userName: data.userName }] };
      }));
    });

    socket.on('reaction_update', (data: { messageId: number; reactions: Omit<Reaction, 'hasReacted'>[] }) => {
      setMessages(prev => prev.map(m => {
        if (m.id !== data.messageId) return m;
        const reactions = data.reactions.map(r => ({
          ...r,
          hasReacted: r.users.includes(currentUser.name),
        }));
        return { ...m, reactions };
      }));
    });

    socket.on('room_created', (room: Room) => {
      setRooms(prev => {
        if (prev.some(r => r.id === room.id)) return prev;
        return [...prev, { ...room, unreadCount: 0 }];
      });
    });

    socket.on('user_online', ({ userId }: { userId: number }) => {
      setOnlineUsers(prev => prev.map(u => u.id === userId ? { ...u, isOnline: true } : u));
    });

    socket.on('user_offline', ({ userId }: { userId: number }) => {
      setOnlineUsers(prev => prev.map(u => u.id === userId ? { ...u, isOnline: false, lastActive: new Date().toISOString() } : u));
    });

    socket.on('user_status_update', ({ userId, status }: { userId: number; status: string }) => {
      setOnlineUsers(prev => prev.map(u => u.id === userId ? { ...u, status } : u));
    });

    socket.on('scheduled_message_created', (msg: Message) => {
      setScheduledMessages(prev => [...prev, msg]);
    });

    return () => {
      socket.off('new_message');
      socket.off('message_deleted');
      socket.off('typing_update');
      socket.off('messages_read');
      socket.off('reaction_update');
      socket.off('room_created');
      socket.off('user_online');
      socket.off('user_offline');
      socket.off('user_status_update');
      socket.off('scheduled_message_created');
    };
  }, [socket, currentUser.id, currentUser.name, currentRoom?.id]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!socket || !currentRoom) return;
    const unread = messages.filter(m => !m.readBy.some(r => r.userId === currentUser.id));
    if (unread.length === 0) return;
    const ids = unread.map(m => m.id);
    socket.emit('mark_read', { roomId: currentRoom.id, userId: currentUser.id, messageIds: ids });
    setRooms(prev => prev.map(r => r.id === currentRoom.id ? { ...r, unreadCount: 0 } : r));
  }, [messages, currentRoom, currentUser.id, socket]);

  const selectRoom = async (room: Room) => {
    setCurrentRoom(room);
    setTypingUsers([]);
    setMessages([]);
    setScheduledMessages([]);

    // Join socket room
    socket?.emit('join_room', { roomId: room.id, userId: currentUser.id });

    // Load messages
    const msgs: Message[] = await fetch(`/api/rooms/${room.id}/messages?userId=${currentUser.id}`).then(r => r.json());
    setMessages(msgs);
    setRooms(prev => prev.map(r => r.id === room.id ? { ...r, unreadCount: 0 } : r));

    // Load scheduled messages for this user
    const sched: Message[] = await fetch(`/api/rooms/${room.id}/scheduled?userId=${currentUser.id}`).then(r => r.json());
    setScheduledMessages(sched);
  };

  const createRoom = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = newRoomName.trim();
    if (!trimmed) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: trimmed, userId: currentUser.id }),
    });
    if (res.ok) {
      const room: Room = await res.json();
      socket?.emit('join_room', { roomId: room.id, userId: currentUser.id });
      setNewRoomName('');
      await loadRooms();
    }
  };

  const joinRoom = async (roomId: number) => {
    await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket?.emit('join_room', { roomId, userId: currentUser.id });
    await loadRooms();
  };

  const sendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentRoom || !messageInput.trim()) return;

    const payload: {
      roomId: number;
      userId: number;
      content: string;
      expiresInMs?: number;
      scheduledFor?: string;
    } = {
      roomId: currentRoom.id,
      userId: currentUser.id,
      content: messageInput.trim(),
    };

    if (ephemeralMs) payload.expiresInMs = ephemeralMs;
    if (showSchedule && scheduleTime) payload.scheduledFor = new Date(scheduleTime).toISOString();

    socket?.emit('send_message', payload);

    setMessageInput('');
    setEphemeralMs(null);
    setShowSchedule(false);
    setScheduleTime('');

    // Stop typing indicator
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    socket?.emit('stop_typing', { roomId: currentRoom.id, userId: currentUser.id, userName: currentUser.name });
  };

  const handleTyping = (e: React.ChangeEvent<HTMLInputElement>) => {
    setMessageInput(e.target.value);
    if (!currentRoom) return;

    const now = Date.now();
    if (now - lastTypingRef.current > 2000) {
      socket?.emit('start_typing', { roomId: currentRoom.id, userId: currentUser.id, userName: currentUser.name });
      lastTypingRef.current = now;
    }

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socket?.emit('stop_typing', { roomId: currentRoom.id, userId: currentUser.id, userName: currentUser.name });
      lastTypingRef.current = 0;
    }, 2000);
  };

  const toggleReaction = (messageId: number, emoji: string) => {
    if (!currentRoom) return;
    socket?.emit('toggle_reaction', { messageId, userId: currentUser.id, emoji, roomId: currentRoom.id });
  };

  const cancelScheduled = async (msgId: number) => {
    await fetch(`/api/messages/${msgId}/schedule`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setScheduledMessages(prev => prev.filter(m => m.id !== msgId));
  };

  const updateStatus = async (status: string) => {
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
    setCurrentUser({ ...currentUser, status });
  };

  const typingText = typingUsers.length === 0 ? null
    : typingUsers.length === 1 ? `${typingUsers[0].userName} is typing...`
    : typingUsers.length === 2 ? `${typingUsers[0].userName} and ${typingUsers[1].userName} are typing...`
    : 'Multiple users are typing...';

  const onlineCount = onlineUsers.filter(u => u.isOnline !== false).length;

  return (
    <div className="chat-layout">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2 className="app-title">PostgreSQL Chat</h2>
          <div className="user-status-area">
            <div className="user-avatar">{currentUser.name[0].toUpperCase()}</div>
            <div className="user-info">
              <span className="user-name-display">{currentUser.name}</span>
              <select
                className="status-select"
                value={currentUser.status}
                onChange={e => updateStatus(e.target.value)}
              >
                <option value="online">🟢 Online</option>
                <option value="away">🟡 Away</option>
                <option value="dnd">🔴 Do Not Disturb</option>
                <option value="invisible">⚫ Invisible</option>
              </select>
            </div>
          </div>
        </div>

        {/* Rooms */}
        <div className="sidebar-section">
          <div className="section-label">ROOMS</div>
          <form onSubmit={createRoom} className="create-room-form">
            <input
              className="room-input"
              type="text"
              placeholder="New room name..."
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              maxLength={50}
            />
            <button className="btn-sm" type="submit">+</button>
          </form>
          <div className="room-list">
            {rooms.map(room => (
              <div
                key={room.id}
                className={`room-item ${currentRoom?.id === room.id ? 'active' : ''}`}
                onClick={() => selectRoom(room)}
              >
                <span className="room-hash">#</span>
                <span className="room-name">{room.name}</span>
                {room.unreadCount > 0 && (
                  <span className="unread-badge">{room.unreadCount}</span>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Online Users */}
        <div className="sidebar-section">
          <div
            className="section-label clickable"
            onClick={() => setShowUserList(!showUserList)}
          >
            USERS ({onlineCount} online) {showUserList ? '▲' : '▼'}
          </div>
          {showUserList && (
            <div className="user-list">
              {onlineUsers.map(user => (
                <div key={user.id} className="user-list-item">
                  <span className={`status-dot status-${user.isOnline === false ? 'offline' : user.status}`} />
                  <span className="user-list-name">{user.name}</span>
                  {user.isOnline === false && (
                    <span className="last-active">{formatRelative(user.lastActive)}</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Main Content */}
      <div className="main-area">
        {!currentRoom ? (
          <div className="empty-state">
            <div className="empty-icon">💬</div>
            <h3>Select a room to start chatting</h3>
            <p>Create a new room or join an existing one</p>
            {rooms.length > 0 && (
              <div className="join-rooms">
                {rooms.map(room => (
                  <button key={room.id} className="btn-primary" onClick={() => joinRoom(room.id)}>
                    Join #{room.name}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          <>
            {/* Room Header */}
            <div className="room-header">
              <span className="room-header-hash">#</span>
              <span className="room-header-name">{currentRoom.name}</span>
            </div>

            {/* Scheduled Messages Banner */}
            {scheduledMessages.length > 0 && (
              <div className="scheduled-banner">
                <span className="scheduled-label">📅 Scheduled ({scheduledMessages.length})</span>
                <div className="scheduled-list">
                  {scheduledMessages.map(msg => (
                    <div key={msg.id} className="scheduled-item">
                      <span className="scheduled-time">
                        {msg.scheduledFor ? new Date(msg.scheduledFor).toLocaleString() : ''}
                      </span>
                      <span className="scheduled-content">{msg.content}</span>
                      <button className="btn-danger-sm" onClick={() => cancelScheduled(msg.id)}>Cancel</button>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Messages */}
            <div className="messages-container">
              {messages.map(msg => (
                <div key={msg.id} className={`message ${msg.userId === currentUser.id ? 'own-message' : ''}`}>
                  <div className="message-header">
                    <span className="message-author">{msg.userName}</span>
                    <span className="message-time">{formatTime(msg.createdAt)}</span>
                    {msg.expiresAt && <EphemeralCountdown expiresAt={msg.expiresAt} />}
                  </div>
                  <div className="message-content">{msg.content}</div>

                  {/* Reactions display */}
                  {msg.reactions.length > 0 && (
                    <div className="reactions-display">
                      {msg.reactions.map(r => (
                        <button
                          key={r.emoji}
                          className={`reaction-chip ${r.hasReacted ? 'reacted' : ''}`}
                          onClick={() => toggleReaction(msg.id, r.emoji)}
                          onMouseEnter={() => setHoveredReaction({ msgId: msg.id, emoji: r.emoji })}
                          onMouseLeave={() => setHoveredReaction(null)}
                          title={r.users.join(', ')}
                        >
                          {r.emoji} {r.count}
                          {hoveredReaction?.msgId === msg.id && hoveredReaction?.emoji === r.emoji && (
                            <span className="reaction-tooltip">{r.users.join(', ')}</span>
                          )}
                        </button>
                      ))}
                    </div>
                  )}

                  {/* Emoji picker row */}
                  <div className="emoji-picker-row">
                    {EMOJIS.map(emoji => (
                      <button
                        key={emoji}
                        className="emoji-btn"
                        onClick={() => toggleReaction(msg.id, emoji)}
                        title={`React with ${emoji}`}
                      >
                        {emoji}
                      </button>
                    ))}
                  </div>

                  {/* Read receipts */}
                  {msg.readBy.filter(r => r.userId !== msg.userId).length > 0 && (
                    <div className="read-receipts">
                      Seen by {msg.readBy.filter(r => r.userId !== msg.userId).map(r => r.userName).join(', ')}
                    </div>
                  )}
                </div>
              ))}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing Indicator */}
            {typingText && (
              <div className="typing-indicator">
                <span className="typing-dots"><span>.</span><span>.</span><span>.</span></span>
                {typingText}
              </div>
            )}

            {/* Message Input */}
            <div className="input-area">
              {/* Options bar */}
              <div className="input-options">
                <button
                  className={`option-btn ${ephemeralMs ? 'active' : ''}`}
                  onClick={() => setEphemeralMs(ephemeralMs ? null : EPHEMERAL_OPTIONS[0].ms)}
                  title="Send ephemeral message"
                >
                  ⏳ Ephemeral
                </button>
                {ephemeralMs && (
                  <select
                    className="ephemeral-select"
                    value={ephemeralMs}
                    onChange={e => setEphemeralMs(Number(e.target.value))}
                  >
                    {EPHEMERAL_OPTIONS.map(opt => (
                      <option key={opt.ms} value={opt.ms}>{opt.label}</option>
                    ))}
                  </select>
                )}
                <button
                  className={`option-btn ${showSchedule ? 'active' : ''}`}
                  onClick={() => setShowSchedule(!showSchedule)}
                  title="Schedule message"
                >
                  📅 Schedule
                </button>
                {showSchedule && (
                  <input
                    type="datetime-local"
                    className="schedule-input"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                    min={new Date().toISOString().slice(0, 16)}
                  />
                )}
              </div>

              <form onSubmit={sendMessage} className="message-form">
                <input
                  className="message-input"
                  type="text"
                  placeholder={`Message #${currentRoom.name}${ephemeralMs ? ' (ephemeral)' : ''}${showSchedule && scheduleTime ? ' (scheduled)' : ''}`}
                  value={messageInput}
                  onChange={handleTyping}
                  maxLength={2000}
                  autoFocus
                />
                <button
                  className="btn-send"
                  type="submit"
                  disabled={!messageInput.trim() || (showSchedule && !scheduleTime)}
                >
                  {showSchedule && scheduleTime ? '📅' : '➤'}
                </button>
              </form>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
