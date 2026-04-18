import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

interface User {
  id: number;
  name: string;
  online: boolean;
}

interface Room {
  id: number;
  name: string;
  unreadCount: number;
  joined: boolean;
}

interface ReadBy {
  userId: number;
  userName: string;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  userName: string;
  content: string;
  createdAt: string;
  readBy: ReadBy[];
}

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [nameError, setNameError] = useState('');
  const [nameLoading, setNameLoading] = useState(false);

  const [rooms, setRooms] = useState<Room[]>([]);
  const [activeRoomId, setActiveRoomId] = useState<number | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [typingUsers, setTypingUsers] = useState<Map<number, string>>(new Map());
  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [connected, setConnected] = useState(false);
  const [messagesLoading, setMessagesLoading] = useState(false);

  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const [roomError, setRoomError] = useState('');

  const [messageInput, setMessageInput] = useState('');
  const [isScrolledUp, setIsScrolledUp] = useState(false);

  const socketRef = useRef<Socket | null>(null);
  const activeRoomIdRef = useRef<number | null>(null);
  const currentUserRef = useRef<User | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep refs in sync
  useEffect(() => { activeRoomIdRef.current = activeRoomId; }, [activeRoomId]);
  useEffect(() => { currentUserRef.current = currentUser; }, [currentUser]);

  const scrollToBottom = useCallback((smooth = true) => {
    messagesEndRef.current?.scrollIntoView({ behavior: smooth ? 'smooth' : 'instant' });
  }, []);

  // Auto-scroll when messages arrive
  useEffect(() => {
    if (!isScrolledUp) scrollToBottom();
  }, [messages, isScrolledUp, scrollToBottom]);

  // ── Socket setup ────────────────────────────────────────────────────────────

  useEffect(() => {
    const socket = io();
    socketRef.current = socket;

    socket.on('connect', () => setConnected(true));
    socket.on('disconnect', () => setConnected(false));

    socket.on('message', (msg: Message) => {
      const roomId = activeRoomIdRef.current;
      const user = currentUserRef.current;

      if (msg.roomId === roomId) {
        setMessages((prev) => [...prev, msg]);
        // Auto-mark as read since user is viewing this room
        if (user) socket.emit('mark_read', { messageId: msg.id });
      } else {
        setRooms((prev) =>
          prev.map((r) => r.id === msg.roomId ? { ...r, unreadCount: r.unreadCount + 1 } : r)
        );
      }
    });

    socket.on('typing', ({ userId, userName, typing }: { userId: number; userName: string; typing: boolean }) => {
      setTypingUsers((prev) => {
        const next = new Map(prev);
        if (typing) next.set(userId, userName);
        else next.delete(userId);
        return next;
      });
    });

    socket.on('read_receipt', ({ messageId, userId, userName }: { messageId: number; userId: number; userName: string }) => {
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id === messageId && m.userId !== userId && !m.readBy.some((r) => r.userId === userId)) {
            return { ...m, readBy: [...m.readBy, { userId, userName }] };
          }
          return m;
        })
      );
    });

    socket.on('bulk_read', ({ messageIds, userId, userName }: { messageIds: number[]; userId: number; userName: string }) => {
      const idSet = new Set(messageIds);
      setMessages((prev) =>
        prev.map((m) => {
          if (idSet.has(m.id) && m.userId !== userId && !m.readBy.some((r) => r.userId === userId)) {
            return { ...m, readBy: [...m.readBy, { userId, userName }] };
          }
          return m;
        })
      );
    });

    socket.on('user_status', ({ userId, online, name }: { userId: number; online: boolean; name: string }) => {
      setOnlineUsers((prev) => {
        if (online) {
          if (prev.some((u) => u.id === userId)) {
            return prev.map((u) => u.id === userId ? { ...u, online: true } : u);
          }
          return [...prev, { id: userId, name, online: true }];
        } else {
          return prev.filter((u) => u.id !== userId);
        }
      });
    });

    socket.on('room_created', (room: Room) => {
      setRooms((prev) => {
        if (prev.some((r) => r.id === room.id)) return prev;
        return [...prev, { ...room, unreadCount: 0, joined: false }];
      });
    });

    return () => { socket.disconnect(); };
  }, []);

  // ── Register with server when user is set ───────────────────────────────────

  useEffect(() => {
    if (!currentUser || !socketRef.current) return;
    const socket = socketRef.current;

    socket.emit('register', { userId: currentUser.id, userName: currentUser.name });

    // Fetch rooms and online users
    fetch(`/api/rooms?userId=${currentUser.id}`)
      .then((r) => r.json())
      .then(setRooms)
      .catch(console.error);

    fetch('/api/users/online')
      .then((r) => r.json())
      .then(setOnlineUsers)
      .catch(console.error);
  }, [currentUser]);

  // ── Join/leave socket room when active room changes ──────────────────────────

  useEffect(() => {
    const socket = socketRef.current;
    const user = currentUser;
    if (!activeRoomId || !user || !socket) return;

    socket.emit('join_room', { roomId: activeRoomId });
    setMessages([]);
    setTypingUsers(new Map());
    setMessagesLoading(true);

    fetch(`/api/rooms/${activeRoomId}/messages?userId=${user.id}`)
      .then((r) => r.json())
      .then((msgs: Message[]) => {
        setMessages(msgs);
        setRooms((prev) => prev.map((r) => r.id === activeRoomId ? { ...r, unreadCount: 0 } : r));
        scrollToBottom(false);
      })
      .catch(console.error)
      .finally(() => setMessagesLoading(false));

    return () => {
      socket.emit('leave_room', { roomId: activeRoomId });
      if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
      socket.emit('typing_stop', { roomId: activeRoomId });
    };
  }, [activeRoomId, currentUser]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Handlers ────────────────────────────────────────────────────────────────

  async function handleSetName(e: React.FormEvent) {
    e.preventDefault();
    const name = nameInput.trim();
    if (!name) return setNameError('Please enter a name');
    if (name.length > 30) return setNameError('Name must be 30 characters or fewer');

    setNameLoading(true);
    setNameError('');
    try {
      const res = await fetch('/api/users', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      });
      const data = await res.json();
      if (!res.ok) return setNameError(data.error ?? 'Failed to set name');
      setCurrentUser(data);
    } catch {
      setNameError('Network error');
    } finally {
      setNameLoading(false);
    }
  }

  async function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    const name = newRoomName.trim();
    if (!name) return setRoomError('Room name required');
    if (!currentUser) return;

    setRoomError('');
    try {
      const res = await fetch('/api/rooms', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, userId: currentUser.id }),
      });
      const data = await res.json();
      if (!res.ok) return setRoomError(data.error ?? 'Failed to create room');
      setRooms((prev) => {
        if (prev.some((r) => r.id === data.id)) return prev;
        return [...prev, data];
      });
      setNewRoomName('');
      setShowCreateRoom(false);
      setActiveRoomId(data.id);
    } catch {
      setRoomError('Network error');
    }
  }

  async function handleJoinRoom(roomId: number) {
    if (!currentUser) return;
    await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: true } : r));
    setActiveRoomId(roomId);
  }

  async function handleLeaveRoom(roomId: number) {
    if (!currentUser) return;
    await fetch(`/api/rooms/${roomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: false } : r));
    if (activeRoomId === roomId) setActiveRoomId(null);
  }

  function handleSelectRoom(room: Room) {
    if (!room.joined) {
      handleJoinRoom(room.id);
    } else {
      setActiveRoomId(room.id);
    }
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    const content = messageInput.trim();
    if (!content || !activeRoomId || !socketRef.current) return;

    socketRef.current.emit('send_message', { roomId: activeRoomId, content });
    setMessageInput('');

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    socketRef.current.emit('typing_stop', { roomId: activeRoomId });
  }

  function handleMessageInput(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageInput(e.target.value);
    if (!activeRoomId || !socketRef.current) return;

    socketRef.current.emit('typing_start', { roomId: activeRoomId });

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socketRef.current?.emit('typing_stop', { roomId: activeRoomId });
    }, 2000);
  }

  function handleScroll() {
    const el = messagesContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    setIsScrolledUp(!atBottom);
  }

  // ── Render ──────────────────────────────────────────────────────────────────

  // Name modal
  if (!currentUser) {
    return (
      <div className="modal-overlay">
        <div className="modal">
          <h2>Welcome to PostgreSQL Chat</h2>
          <p>Enter a display name to get started.</p>
          <form onSubmit={handleSetName} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div className="form-field">
              <label>Display Name</label>
              <input
                className="text-input"
                type="text"
                placeholder="e.g. Alice"
                value={nameInput}
                onChange={(e) => setNameInput(e.target.value)}
                maxLength={30}
                autoFocus
              />
              {nameError && <span className="error-msg">{nameError}</span>}
            </div>
            <button className="btn btn-primary" type="submit" disabled={nameLoading || !nameInput.trim()}>
              {nameLoading ? 'Joining…' : 'Enter Chat'}
            </button>
          </form>
        </div>
      </div>
    );
  }

  const activeRoom = rooms.find((r) => r.id === activeRoomId) ?? null;
  const typingList = Array.from(typingUsers.values()).filter((n) => n !== currentUser.name);

  let typingText = '';
  if (typingList.length === 1) typingText = `${typingList[0]} is typing`;
  else if (typingList.length === 2) typingText = `${typingList[0]} and ${typingList[1]} are typing`;
  else if (typingList.length > 2) typingText = 'Multiple users are typing';

  // Group consecutive messages from same sender (within 5 min)
  const groupedMessages: { msgs: Message[]; grouped: boolean }[] = [];
  for (const msg of messages) {
    const last = groupedMessages[groupedMessages.length - 1];
    const prevMsg = last?.msgs[last.msgs.length - 1];
    if (
      last &&
      prevMsg &&
      prevMsg.userId === msg.userId &&
      new Date(msg.createdAt).getTime() - new Date(prevMsg.createdAt).getTime() < 5 * 60 * 1000
    ) {
      last.msgs.push(msg);
    } else {
      groupedMessages.push({ msgs: [msg], grouped: false });
    }
  }

  const formatTime = (iso: string) => {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  return (
    <div className="app">
      {/* ── Sidebar ── */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1>PostgreSQL Chat</h1>
          <p>Real-time powered by Postgres</p>
        </div>

        <div className="sidebar-scrollable">
          <div className="sidebar-section">
            <div className="sidebar-section-title">Rooms</div>
            {rooms.length === 0 && (
              <div style={{ padding: '8px 16px', fontSize: 13, color: 'var(--text-muted)' }}>
                No rooms yet
              </div>
            )}
            {rooms.map((room) => (
              <div
                key={room.id}
                className={`room-item ${activeRoomId === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room)}
              >
                <div className="room-item-name">
                  <span style={{ color: 'var(--text-muted)', fontSize: 13 }}>#</span>
                  <span>{room.name}</span>
                  {!room.joined && (
                    <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 2 }}>+join</span>
                  )}
                </div>
                {room.unreadCount > 0 && (
                  <span className="unread-badge">{room.unreadCount}</span>
                )}
              </div>
            ))}
          </div>

          {showCreateRoom ? (
            <form className="create-room-form" onSubmit={handleCreateRoom}>
              <input
                type="text"
                placeholder="Room name"
                value={newRoomName}
                onChange={(e) => setNewRoomName(e.target.value)}
                maxLength={50}
                autoFocus
                onKeyDown={(e) => { if (e.key === 'Escape') { setShowCreateRoom(false); setRoomError(''); } }}
              />
              {roomError && <span className="error-msg">{roomError}</span>}
              <div className="create-room-form-actions">
                <button className="btn btn-primary btn-sm" type="submit">Create</button>
                <button className="btn btn-ghost btn-sm" type="button" onClick={() => { setShowCreateRoom(false); setRoomError(''); }}>Cancel</button>
              </div>
            </form>
          ) : (
            <div style={{ padding: '4px 16px 8px' }}>
              <button className="btn btn-ghost btn-sm" onClick={() => setShowCreateRoom(true)}>
                + New Room
              </button>
            </div>
          )}

          <div className="sidebar-section" style={{ borderTop: '1px solid var(--border)', marginTop: 4 }}>
            <div className="sidebar-section-title">Online ({onlineUsers.length})</div>
            {onlineUsers.map((u) => (
              <div key={u.id} className="user-item">
                <span className="status-dot online" />
                <span style={{ color: u.id === currentUser.id ? 'var(--accent)' : 'var(--text-muted)' }}>
                  {u.name}{u.id === currentUser.id ? ' (you)' : ''}
                </span>
              </div>
            ))}
          </div>
        </div>

        <div className="sidebar-user-info">
          <span className="status-dot online" />
          <span className="user-name">{currentUser.name}</span>
          {!connected && <span style={{ fontSize: 11, color: 'var(--warning)', marginLeft: 'auto' }}>●</span>}
        </div>
      </aside>

      {/* ── Main Area ── */}
      <main className="main-area">
        {!connected && (
          <div className="connection-banner">Reconnecting…</div>
        )}

        {!activeRoom ? (
          <div className="empty-state">
            <h2>PostgreSQL Chat</h2>
            <p>Select a room from the sidebar to start chatting,<br />or create a new one.</p>
            {rooms.length === 0 && (
              <button className="btn btn-primary" onClick={() => setShowCreateRoom(true)}>
                Create a Room
              </button>
            )}
          </div>
        ) : (
          <>
            <div className="room-header">
              <span className="room-header-name"># {activeRoom.name}</span>
              <div className="room-header-actions">
                {activeRoom.joined && (
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => handleLeaveRoom(activeRoom.id)}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            <div
              className="messages-container"
              ref={messagesContainerRef}
              onScroll={handleScroll}
              style={{ position: 'relative' }}
            >
              {messagesLoading ? (
                <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
                  <div className="connecting-state">
                    <div className="spinner" />
                    Loading messages…
                  </div>
                </div>
              ) : messages.length === 0 ? (
                <div className="empty-state" style={{ flex: 'none', marginTop: 40 }}>
                  <p>No messages yet. Say hello!</p>
                </div>
              ) : (
                groupedMessages.map(({ msgs }) => {
                  const first = msgs[0];
                  const isOwn = first.userId === currentUser.id;
                  return (
                    <div key={first.id} className="message-group">
                      <div className="message-header">
                        <span
                          className="message-sender"
                          style={{ color: isOwn ? 'var(--warning)' : 'var(--accent)' }}
                        >
                          {first.userName}
                        </span>
                        <span className="message-time">{formatTime(first.createdAt)}</span>
                      </div>
                      {msgs.map((msg) => (
                        <div key={msg.id}>
                          <div className={`message-row ${isOwn ? 'own' : ''}`}>
                            <div className="message-content">{msg.content}</div>
                          </div>
                          {msg.readBy.length > 0 && (
                            <div className="read-receipts">
                              <span className="read-receipts-icon">✓✓</span>
                              Seen by {msg.readBy.map((r) => r.userName).join(', ')}
                            </div>
                          )}
                        </div>
                      ))}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {isScrolledUp && (
              <button
                className="scroll-to-bottom"
                onClick={() => { scrollToBottom(); setIsScrolledUp(false); }}
              >
                ↓ New messages
              </button>
            )}

            <div className="typing-indicator">
              {typingText && (
                <>
                  {typingText}
                  <span className="typing-dots">
                    <span /><span /><span />
                  </span>
                </>
              )}
            </div>

            <form className="input-bar" onSubmit={handleSendMessage}>
              <input
                className="message-input"
                type="text"
                placeholder={`Message #${activeRoom.name}`}
                value={messageInput}
                onChange={handleMessageInput}
                maxLength={2000}
                autoComplete="off"
              />
              <button
                className="btn btn-primary"
                type="submit"
                disabled={!messageInput.trim()}
              >
                Send
              </button>
            </form>
          </>
        )}
      </main>
    </div>
  );
}
