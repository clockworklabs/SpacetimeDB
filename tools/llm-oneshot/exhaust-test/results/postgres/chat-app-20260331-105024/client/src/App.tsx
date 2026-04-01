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
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  userName: string;
  content: string;
  createdAt: string;
  seenBy: string[];
}

function formatTime(dateStr: string): string {
  const d = new Date(dateStr);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(() => {
    const id = localStorage.getItem('userId');
    const name = localStorage.getItem('userName');
    if (id && name) return { id: parseInt(id), name, online: true };
    return null;
  });
  const [registerName, setRegisterName] = useState('');
  const [registerError, setRegisterError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoom, setCurrentRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [users, setUsers] = useState<User[]>([]);
  const [typingUsers, setTypingUsers] = useState<{ userId: number; userName: string }[]>([]);
  const [newMessage, setNewMessage] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showNewRoom, setShowNewRoom] = useState(false);
  const [sendError, setSendError] = useState('');

  const socketRef = useRef<Socket | null>(null);
  const currentRoomRef = useRef<Room | null>(null);
  const currentUserRef = useRef<User | null>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);

  // Keep refs in sync with state
  useEffect(() => { currentRoomRef.current = currentRoom; }, [currentRoom]);
  useEffect(() => { currentUserRef.current = currentUser; }, [currentUser]);

  // Auto-scroll to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Initialize socket
  useEffect(() => {
    const socket = io();
    socketRef.current = socket;

    socket.on('message', (msg: Message) => {
      const room = currentRoomRef.current;
      const user = currentUserRef.current;
      if (room && msg.roomId === room.id) {
        setMessages(prev => [...prev, msg]);
        // Auto-mark as read
        if (user) {
          socket.emit('read_up_to', { roomId: msg.roomId, lastMessageId: msg.id });
        }
      } else {
        setRooms(prev => prev.map(r =>
          r.id === msg.roomId ? { ...r, unreadCount: r.unreadCount + 1 } : r
        ));
      }
    });

    socket.on('typing', ({ userId, userName, roomId }: { userId: number; userName: string; roomId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setTypingUsers(prev => {
          if (prev.find(u => u.userId === userId)) return prev;
          return [...prev, { userId, userName }];
        });
      }
    });

    socket.on('typing_stop', ({ userId, roomId }: { userId: number; roomId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setTypingUsers(prev => prev.filter(u => u.userId !== userId));
      }
    });

    socket.on('user_status', ({ userId, online }: { userId: number; online: boolean }) => {
      setUsers(prev => prev.map(u => u.id === userId ? { ...u, online } : u));
    });

    socket.on('read_update', ({ userId, userName, roomId, lastMessageId }: {
      userId: number; userName: string; roomId: number; lastMessageId: number
    }) => {
      if (currentRoomRef.current?.id === roomId) {
        setMessages(prev => prev.map(m => {
          if (m.id <= lastMessageId && m.userId !== userId) {
            if (m.seenBy.includes(userName)) return m;
            return { ...m, seenBy: [...m.seenBy, userName] };
          }
          return m;
        }));
      }
    });

    socket.on('unread_update', ({ roomId, count }: { roomId: number; count: number }) => {
      setRooms(prev => prev.map(r => r.id === roomId ? { ...r, unreadCount: count } : r));
    });

    socket.on('room_created', (room: Room & { unreadCount: number }) => {
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room].sort((a, b) => a.name.localeCompare(b.name));
      });
    });

    return () => { socket.disconnect(); };
  }, []);

  // Identify with socket when user logs in
  useEffect(() => {
    if (socketRef.current && currentUser) {
      socketRef.current.emit('identify', currentUser.id);
    }
  }, [currentUser]);

  // Fetch initial data when logged in
  useEffect(() => {
    if (!currentUser) return;
    Promise.all([
      fetch(`/api/rooms?userId=${currentUser.id}`).then(r => r.json()),
      fetch('/api/users').then(r => r.json()),
    ]).then(([roomsData, usersData]) => {
      setRooms(roomsData);
      setUsers(usersData);
    });
  }, [currentUser]);

  const handleRegister = useCallback(async () => {
    if (!registerName.trim()) return;
    setRegisterError('');

    const res = await fetch('/api/users/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: registerName.trim() }),
    });

    if (res.ok) {
      const user: User = await res.json();
      setCurrentUser(user);
      localStorage.setItem('userId', String(user.id));
      localStorage.setItem('userName', user.name);
    } else {
      const err = await res.json();
      setRegisterError(err.error || 'Registration failed');
    }
  }, [registerName]);

  const handleJoinRoom = useCallback(async (room: Room) => {
    if (!currentUser) return;

    // Leave previous room socket subscription
    if (currentRoomRef.current) {
      socketRef.current?.emit('leave_room', currentRoomRef.current.id);
    }

    await fetch(`/api/rooms/${room.id}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });

    socketRef.current?.emit('join_room', room.id);

    const res = await fetch(`/api/rooms/${room.id}/messages?userId=${currentUser.id}`);
    const msgs: Message[] = await res.json();
    setMessages(msgs);
    setTypingUsers([]);
    setCurrentRoom({ ...room, unreadCount: 0 });
    setRooms(prev => prev.map(r => r.id === room.id ? { ...r, unreadCount: 0 } : r));

    if (msgs.length > 0) {
      socketRef.current?.emit('read_up_to', { roomId: room.id, lastMessageId: msgs[msgs.length - 1].id });
    }
  }, [currentUser]);

  const handleSendMessage = useCallback(async () => {
    if (!currentUser || !currentRoom || !newMessage.trim()) return;
    const content = newMessage.trim();
    setNewMessage('');
    setSendError('');

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);

    const res = await fetch(`/api/rooms/${currentRoom.id}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content }),
    });

    if (!res.ok) {
      const err = await res.json();
      setSendError(err.error || 'Failed to send');
      setNewMessage(content);
    }
  }, [currentUser, currentRoom, newMessage]);

  const handleTyping = useCallback(() => {
    if (!currentUser || !currentRoom) return;
    socketRef.current?.emit('typing', currentRoom.id);
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {}, 2500);
  }, [currentUser, currentRoom]);

  const handleCreateRoom = useCallback(async () => {
    if (!currentUser || !newRoomName.trim()) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    if (res.ok) {
      const room: Room = await res.json();
      setNewRoomName('');
      setShowNewRoom(false);
      handleJoinRoom({ ...room, unreadCount: 0 });
    }
  }, [currentUser, newRoomName, handleJoinRoom]);

  const handleLogout = useCallback(() => {
    localStorage.removeItem('userId');
    localStorage.removeItem('userName');
    socketRef.current?.disconnect();
    setCurrentUser(null);
    setCurrentRoom(null);
    setMessages([]);
    setRooms([]);
  }, []);

  const typingText = typingUsers.length === 1
    ? `${typingUsers[0].userName} is typing...`
    : typingUsers.length > 1
    ? `${typingUsers.map(u => u.userName).join(', ')} are typing...`
    : '';

  // Registration screen
  if (!currentUser) {
    return (
      <div className="auth-screen">
        <div className="auth-card">
          <div className="logo">
            <span className="logo-icon">⬡</span>
            <h1>SpacetimeDB Chat</h1>
          </div>
          <p className="auth-subtitle">Enter a display name to get started</p>
          <div className="auth-form">
            <input
              type="text"
              placeholder="Your display name"
              value={registerName}
              onChange={e => setRegisterName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleRegister()}
              maxLength={32}
              autoFocus
              className="auth-input"
            />
            {registerError && <div className="error-msg">{registerError}</div>}
            <button onClick={handleRegister} className="btn-primary" disabled={!registerName.trim()}>
              Join Chat
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* Left sidebar: rooms */}
      <div className="sidebar rooms-sidebar">
        <div className="sidebar-header">
          <div className="sidebar-title">Rooms</div>
          <button className="btn-icon" onClick={() => setShowNewRoom(v => !v)} title="New room">+</button>
        </div>
        {showNewRoom && (
          <div className="new-room-form">
            <input
              type="text"
              placeholder="Room name"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
              maxLength={64}
              autoFocus
              className="room-input"
            />
            <button onClick={handleCreateRoom} className="btn-small" disabled={!newRoomName.trim()}>
              Create
            </button>
          </div>
        )}
        <div className="room-list">
          {rooms.map(room => (
            <div
              key={room.id}
              className={`room-item ${currentRoom?.id === room.id ? 'active' : ''}`}
              onClick={() => handleJoinRoom(room)}
            >
              <span className="room-hash">#</span>
              <span className="room-name">{room.name}</span>
              {room.unreadCount > 0 && (
                <span className="unread-badge">{room.unreadCount > 99 ? '99+' : room.unreadCount}</span>
              )}
            </div>
          ))}
          {rooms.length === 0 && (
            <div className="empty-hint">No rooms yet. Create one!</div>
          )}
        </div>
        <div className="sidebar-footer">
          <div className="current-user">
            <span className="online-dot" />
            <span className="user-name">{currentUser.name}</span>
          </div>
          <button className="btn-logout" onClick={handleLogout} title="Leave">✕</button>
        </div>
      </div>

      {/* Center: chat */}
      <div className="chat-main">
        {currentRoom ? (
          <>
            <div className="chat-header">
              <span className="chat-room-name"># {currentRoom.name}</span>
            </div>
            <div className="messages-container">
              {messages.length === 0 && (
                <div className="empty-messages">No messages yet. Say hello!</div>
              )}
              {messages.map((msg, i) => {
                const isOwn = msg.userId === currentUser.id;
                const prevMsg = messages[i - 1];
                const showHeader = !prevMsg || prevMsg.userId !== msg.userId;
                return (
                  <div key={msg.id} className={`message-group ${isOwn ? 'own' : ''}`}>
                    {showHeader && (
                      <div className="message-header">
                        <span className="message-author">{msg.userName}</span>
                        <span className="message-time">{formatTime(msg.createdAt)}</span>
                      </div>
                    )}
                    <div className="message-bubble">{msg.content}</div>
                    {msg.seenBy.length > 0 && (
                      <div className="seen-by">
                        Seen by {msg.seenBy.join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>
            <div className="typing-indicator">
              {typingText && <span className="typing-text">{typingText}</span>}
            </div>
            <div className="message-input-area">
              {sendError && <div className="error-msg send-error">{sendError}</div>}
              <div className="message-input-row">
                <input
                  type="text"
                  placeholder={`Message #${currentRoom.name}`}
                  value={newMessage}
                  onChange={e => { setNewMessage(e.target.value); handleTyping(); }}
                  onKeyDown={e => e.key === 'Enter' && !e.shiftKey && handleSendMessage()}
                  maxLength={2000}
                  className="message-input"
                />
                <button
                  onClick={handleSendMessage}
                  disabled={!newMessage.trim()}
                  className="btn-send"
                >
                  Send
                </button>
              </div>
            </div>
          </>
        ) : (
          <div className="no-room">
            <div className="no-room-content">
              <span className="logo-icon large">⬡</span>
              <h2>Welcome, {currentUser.name}!</h2>
              <p>Select a room to start chatting, or create a new one.</p>
            </div>
          </div>
        )}
      </div>

      {/* Right sidebar: online users */}
      <div className="sidebar users-sidebar">
        <div className="sidebar-header">
          <div className="sidebar-title">Online</div>
        </div>
        <div className="user-list">
          {users
            .filter(u => u.online)
            .map(u => (
              <div key={u.id} className="user-item">
                <span className="online-dot" />
                <span className="user-item-name">{u.name}</span>
                {u.id === currentUser.id && <span className="you-label">(you)</span>}
              </div>
            ))}
        </div>
        {users.filter(u => !u.online).length > 0 && (
          <>
            <div className="users-section-label">Offline</div>
            <div className="user-list">
              {users
                .filter(u => !u.online)
                .map(u => (
                  <div key={u.id} className="user-item offline">
                    <span className="offline-dot" />
                    <span className="user-item-name">{u.name}</span>
                  </div>
                ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
