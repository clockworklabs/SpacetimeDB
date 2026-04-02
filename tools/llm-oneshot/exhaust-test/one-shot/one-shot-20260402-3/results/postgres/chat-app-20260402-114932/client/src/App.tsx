import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ─── Types ────────────────────────────────────────────────────────────────────

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
}

interface Reaction {
  emoji: string;
  count: number;
  users: string[];
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
  isEdited: boolean;
  expiresAt: string | null;
  createdAt: string;
  reactions: Reaction[];
  readBy: ReadBy[];
}

interface RoomMember {
  userId: number;
  name: string;
  isAdmin: boolean;
  isBanned: boolean;
  status: string;
  lastActive: string;
  isOnline: boolean;
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  scheduledFor: string;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '🔥'];

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function timeAgo(iso: string): string {
  const diff = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function statusLabel(status: string): string {
  switch (status) {
    case 'online': return '🟢 Online';
    case 'away': return '🟡 Away';
    case 'dnd': return '🔴 Do Not Disturb';
    case 'invisible': return '⚫ Invisible';
    default: return '⚫ Offline';
  }
}

function getEphemeralRemaining(expiresAt: string): string {
  const remaining = Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000);
  if (remaining <= 0) return 'Expiring...';
  if (remaining < 60) return `Disappears in ${remaining}s`;
  return `Disappears in ${Math.floor(remaining / 60)}m`;
}

// ─── App ──────────────────────────────────────────────────────────────────────

export default function App() {
  const [socket, setSocket] = useState<Socket | null>(null);
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [loginError, setLoginError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoom, setCurrentRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [members, setMembers] = useState<RoomMember[]>([]);
  const [typingUsers, setTypingUsers] = useState<string[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);

  const [newRoomName, setNewRoomName] = useState('');
  const [messageInput, setMessageInput] = useState('');
  const [ephemeralSeconds, setEphemeralSeconds] = useState<number>(0);
  const [scheduleTime, setScheduleTime] = useState('');
  const [showSchedule, setShowSchedule] = useState(false);

  const [editingMessage, setEditingMessage] = useState<Message | null>(null);
  const [editContent, setEditContent] = useState('');
  const [showHistory, setShowHistory] = useState<{ messageId: number; edits: { content: string; editedAt: string }[] } | null>(null);
  const [showEmojiFor, setShowEmojiFor] = useState<number | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const currentRoomRef = useRef<Room | null>(null);
  const currentUserRef = useRef<User | null>(null);

  // Tick to update ephemeral countdowns
  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // Sync refs
  useEffect(() => { currentRoomRef.current = currentRoom; }, [currentRoom]);
  useEffect(() => { currentUserRef.current = currentUser; }, [currentUser]);

  // Activity ping
  useEffect(() => {
    if (!socket || !currentUser) return;
    const handler = () => {
      socket.emit('activity_ping');
    };
    window.addEventListener('mousemove', handler);
    window.addEventListener('keydown', handler);
    return () => {
      window.removeEventListener('mousemove', handler);
      window.removeEventListener('keydown', handler);
    };
  }, [socket, currentUser]);

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Mark messages read when room changes or new messages arrive
  const markRead = useCallback((roomId: number, messageId: number) => {
    if (!socket) return;
    socket.emit('mark_read', { roomId, messageId });
  }, [socket]);

  useEffect(() => {
    if (!currentRoom || messages.length === 0 || !socket) return;
    const lastMsg = messages[messages.length - 1];
    markRead(currentRoom.id, lastMsg.id);
  }, [messages, currentRoom, markRead, socket]);

  // Socket setup
  useEffect(() => {
    const s = io({ path: '/socket.io', transports: ['websocket', 'polling'] });
    setSocket(s);

    s.on('authenticated', (user: User) => {
      setCurrentUser(user);
    });

    s.on('room_joined', ({ room, messages: msgs, members: mbrs }: {
      room: Room; messages: Message[]; members: RoomMember[];
    }) => {
      setCurrentRoom(room);
      setMessages(msgs);
      setMembers(mbrs);
      setTypingUsers([]);
    });

    s.on('new_message', (msg: Message) => {
      setMessages((prev) => {
        if (prev.find((m) => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      // Mark read if we're in this room
      if (currentRoomRef.current?.id === msg.roomId) {
        setTimeout(() => markRead(msg.roomId, msg.id), 200);
      }
    });

    s.on('message_updated', (msg: Message) => {
      setMessages((prev) => prev.map((m) => m.id === msg.id ? msg : m));
    });

    s.on('message_deleted', ({ messageId }: { messageId: number }) => {
      setMessages((prev) => prev.filter((m) => m.id !== messageId));
    });

    s.on('typing_update', ({ roomId, typingUsers: users }: { roomId: number; typingUsers: string[] }) => {
      if (currentRoomRef.current?.id === roomId) {
        setTypingUsers(users.filter((n) => n !== currentUserRef.current?.name));
      }
    });

    s.on('read_receipt_update', ({ messageId, seenBy }: { messageId: number; seenBy: ReadBy[] }) => {
      setMessages((prev) => prev.map((m) => m.id === messageId ? { ...m, readBy: seenBy } : m));
    });

    s.on('unread_update', ({ roomId, count }: { roomId: number; count: number }) => {
      if (currentRoomRef.current?.id !== roomId) {
        setUnreadCounts((prev) => ({ ...prev, [roomId]: count }));
      }
    });

    s.on('reaction_update', ({ messageId, reactions }: { messageId: number; reactions: Reaction[] }) => {
      setMessages((prev) => prev.map((m) => m.id === messageId ? { ...m, reactions } : m));
    });

    s.on('users_online', (users: User[]) => {
      setOnlineUsers(users);
    });

    s.on('presence_update', ({ userId, status, lastActive }: { userId: number; name: string; status: string; lastActive: string }) => {
      setOnlineUsers((prev) => prev.map((u) => u.id === userId ? { ...u, status, lastActive, isOnline: status !== 'offline' } : u));
    });

    s.on('you_kicked', ({ roomId }: { roomId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setCurrentRoom(null);
        setMessages([]);
        setMembers([]);
      }
      alert('You were kicked from the room!');
    });

    s.on('you_banned', ({ roomId }: { roomId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setCurrentRoom(null);
        setMessages([]);
        setMembers([]);
      }
      alert('You were banned from the room!');
    });

    s.on('user_kicked', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setMembers((prev) => prev.filter((m) => m.userId !== userId));
      }
    });

    s.on('user_banned', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setMembers((prev) => prev.map((m) => m.userId === userId ? { ...m, isBanned: true } : m));
      }
    });

    s.on('member_promoted', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (currentRoomRef.current?.id === roomId) {
        setMembers((prev) => prev.map((m) => m.userId === userId ? { ...m, isAdmin: true } : m));
      }
    });

    s.on('scheduled_sent', ({ id }: { id: number }) => {
      setScheduledMessages((prev) => prev.filter((sm) => sm.id !== id));
    });

    s.on('scheduled_cancelled', ({ id }: { id: number }) => {
      setScheduledMessages((prev) => prev.filter((sm) => sm.id !== id));
    });

    return () => { s.disconnect(); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Load rooms
  useEffect(() => {
    if (!currentUser) return;
    fetch('/api/rooms')
      .then((r) => r.json())
      .then((data: Room[]) => setRooms(data));
  }, [currentUser]);

  // ─── Handlers ────────────────────────────────────────────────────────────────

  const handleLogin = async () => {
    if (!nameInput.trim()) return;
    setLoginError('');
    const res = await fetch('/api/users/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: nameInput.trim() }),
    });
    if (!res.ok) {
      setLoginError('Registration failed');
      return;
    }
    const user: User = await res.json();
    setCurrentUser(user);
    socket?.emit('authenticate', { userId: user.id });
  };

  const handleCreateRoom = async () => {
    if (!newRoomName.trim() || !currentUser) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    if (!res.ok) return;
    const room: Room = await res.json();
    setRooms((prev) => [...prev, room]);
    setNewRoomName('');
    handleJoinRoom(room);
  };

  const handleJoinRoom = async (room: Room) => {
    if (!currentUser || !socket) return;
    if (currentRoom?.id === room.id) return;

    if (currentRoom) {
      socket.emit('leave_room', { roomId: currentRoom.id });
    }

    // Join via REST first (ensures DB membership)
    await fetch(`/api/rooms/${room.id}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });

    socket.emit('join_room', { roomId: room.id });
    setUnreadCounts((prev) => ({ ...prev, [room.id]: 0 }));

    // Load scheduled messages for this room
    const res = await fetch(`/api/rooms/${room.id}/scheduled?userId=${currentUser.id}`);
    if (res.ok) {
      const scheduled: ScheduledMessage[] = await res.json();
      setScheduledMessages(scheduled);
    }
  };

  const handleSendMessage = () => {
    if (!messageInput.trim() || !currentRoom || !socket) return;

    if (showSchedule && scheduleTime) {
      // Schedule the message
      fetch(`/api/rooms/${currentRoom.id}/scheduled`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          userId: currentUser!.id,
          content: messageInput.trim(),
          scheduledFor: new Date(scheduleTime).toISOString(),
        }),
      })
        .then((r) => r.json())
        .then((sm: ScheduledMessage) => {
          setScheduledMessages((prev) => [...prev, sm]);
          setMessageInput('');
          setScheduleTime('');
          setShowSchedule(false);
        });
      return;
    }

    socket.emit('send_message', {
      roomId: currentRoom.id,
      content: messageInput.trim(),
      expiresInSeconds: ephemeralSeconds > 0 ? ephemeralSeconds : undefined,
    });

    // Stop typing
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    socket.emit('typing_stop', { roomId: currentRoom.id });

    setMessageInput('');
  };

  const handleInputChange = (value: string) => {
    setMessageInput(value);
    if (!currentRoom || !socket) return;
    socket.emit('typing_start', { roomId: currentRoom.id });
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socket.emit('typing_stop', { roomId: currentRoom.id });
    }, 3000);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  };

  const handleEditMessage = (msg: Message) => {
    setEditingMessage(msg);
    setEditContent(msg.content);
  };

  const handleSubmitEdit = () => {
    if (!editingMessage || !editContent.trim() || !socket) return;
    socket.emit('edit_message', { messageId: editingMessage.id, content: editContent.trim() });
    setEditingMessage(null);
  };

  const handleShowHistory = async (messageId: number) => {
    const res = await fetch(`/api/messages/${messageId}/history`);
    const edits: { content: string; editedAt: string }[] = await res.json();
    setShowHistory({ messageId, edits });
  };

  const handleReaction = (messageId: number, emoji: string) => {
    if (!socket) return;
    socket.emit('add_reaction', { messageId, emoji });
    setShowEmojiFor(null);
  };

  const handleCancelScheduled = async (id: number) => {
    if (!currentUser) return;
    await fetch(`/api/scheduled/${id}?userId=${currentUser.id}`, { method: 'DELETE' });
  };

  const handleSetStatus = (status: string) => {
    if (!socket) return;
    socket.emit('set_status', { status });
    setCurrentUser((prev) => prev ? { ...prev, status } : prev);
  };

  const handleLeaveRoom = () => {
    if (!currentRoom || !socket || !currentUser) return;
    fetch(`/api/rooms/${currentRoom.id}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket.emit('leave_room', { roomId: currentRoom.id });
    setCurrentRoom(null);
    setMessages([]);
    setMembers([]);
  };

  const handleKick = async (targetUserId: number) => {
    if (!currentRoom || !currentUser) return;
    await fetch(`/api/rooms/${currentRoom.id}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
  };

  const handleBan = async (targetUserId: number) => {
    if (!currentRoom || !currentUser) return;
    await fetch(`/api/rooms/${currentRoom.id}/ban`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
  };

  const handlePromote = async (targetUserId: number) => {
    if (!currentRoom || !currentUser) return;
    await fetch(`/api/rooms/${currentRoom.id}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
  };

  const myMember = members.find((m) => m.userId === currentUser?.id);
  const amAdmin = myMember?.isAdmin ?? false;

  // ─── Login Screen ─────────────────────────────────────────────────────────────

  if (!currentUser) {
    return (
      <div className="login-container">
        <div className="login-box">
          <h1>PostgreSQL Chat</h1>
          <p>Enter your display name to get started</p>
          <input
            type="text"
            placeholder="Your name"
            value={nameInput}
            onChange={(e) => setNameInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleLogin()}
            maxLength={50}
            autoFocus
          />
          {loginError && <div className="error-msg">{loginError}</div>}
          <button className="btn-primary" onClick={handleLogin}>
            Join Chat
          </button>
        </div>
      </div>
    );
  }

  // ─── Main UI ──────────────────────────────────────────────────────────────────

  return (
    <div className="app-layout">
      {/* Header */}
      <header className="app-header">
        <h1>PostgreSQL Chat</h1>
        <div className="header-user">
          <span>Logged in as <strong>{currentUser.name}</strong></span>
          <select
            className="status-select"
            value={currentUser.status}
            onChange={(e) => handleSetStatus(e.target.value)}
          >
            <option value="online">🟢 Online</option>
            <option value="away">🟡 Away</option>
            <option value="dnd">🔴 Do Not Disturb</option>
            <option value="invisible">⚫ Invisible</option>
          </select>
        </div>
      </header>

      <div className="main-content">
        {/* Sidebar */}
        <aside className="sidebar">
          <div className="sidebar-section">
            <h3>Rooms</h3>
            <div className="room-list">
              {rooms.map((room) => (
                <div
                  key={room.id}
                  className={`room-item ${currentRoom?.id === room.id ? 'active' : ''}`}
                  onClick={() => handleJoinRoom(room)}
                >
                  <span className="room-name"># {room.name}</span>
                  {(unreadCounts[room.id] ?? 0) > 0 && currentRoom?.id !== room.id && (
                    <span className="unread-badge">{unreadCounts[room.id]}</span>
                  )}
                </div>
              ))}
            </div>
            <div className="create-room">
              <input
                type="text"
                placeholder="New room name"
                value={newRoomName}
                onChange={(e) => setNewRoomName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleCreateRoom()}
                maxLength={100}
              />
              <button className="btn-small" onClick={handleCreateRoom}>+</button>
            </div>
          </div>

          <div className="sidebar-section" style={{ flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
            <h3>Users</h3>
            <div className="users-list">
              {onlineUsers.map((u) => (
                <div key={u.id} className="user-item">
                  <span className={`status-dot ${u.isOnline ? u.status : 'offline'}`} title={statusLabel(u.isOnline ? u.status : 'offline')} />
                  <div>
                    <div className="user-name">{u.name}{u.id === currentUser.id ? ' (you)' : ''}</div>
                    {!u.isOnline && u.lastActive && (
                      <div className="user-last-active">Last active {timeAgo(u.lastActive)}</div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </aside>

        {/* Chat area */}
        <div className="chat-area">
          {currentRoom ? (
            <>
              <div className="chat-header">
                <h2># {currentRoom.name}</h2>
                <div className="chat-header-actions">
                  <button className="btn-danger" onClick={handleLeaveRoom}>Leave</button>
                </div>
              </div>

              {/* Scheduled messages panel */}
              {scheduledMessages.length > 0 && (
                <div className="scheduled-panel">
                  <h4>Scheduled Messages</h4>
                  {scheduledMessages.map((sm) => (
                    <div key={sm.id} className="scheduled-item">
                      <span className="scheduled-content">{sm.content}</span>
                      <span className="scheduled-time">
                        {new Date(sm.scheduledFor).toLocaleString()}
                      </span>
                      <button className="btn-small" style={{ background: 'var(--danger)' }} onClick={() => handleCancelScheduled(sm.id)}>
                        Cancel
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <div className="messages-container" onClick={() => setShowEmojiFor(null)}>
                {messages.map((msg) => {
                  const isOwn = msg.userId === currentUser.id;
                  const isEphemeral = !!msg.expiresAt;
                  return (
                    <div key={msg.id} className={`message ${isEphemeral ? 'ephemeral' : ''}`}>
                      <div className="message-header">
                        <span className="message-author">{msg.userName}</span>
                        <span className="message-time">{formatTime(msg.createdAt)}</span>
                        {msg.isEdited && (
                          <span
                            className="message-edited"
                            style={{ cursor: 'pointer', textDecoration: 'underline' }}
                            onClick={() => handleShowHistory(msg.id)}
                            title="View edit history"
                          >
                            (edited)
                          </span>
                        )}
                      </div>
                      <div className="message-content">{msg.content}</div>
                      {isEphemeral && msg.expiresAt && (
                        <div className="ephemeral-indicator">
                          ⏱ {getEphemeralRemaining(msg.expiresAt)}
                        </div>
                      )}

                      {/* Reactions */}
                      {msg.reactions.length > 0 && (
                        <div className="message-reactions">
                          {msg.reactions.map((rxn) => {
                            const userReacted = rxn.users.includes(currentUser.name);
                            return (
                              <button
                                key={rxn.emoji}
                                className={`reaction-btn ${userReacted ? 'own' : ''}`}
                                onClick={(e) => { e.stopPropagation(); handleReaction(msg.id, rxn.emoji); }}
                                title={`Reacted by: ${rxn.users.join(', ')}`}
                              >
                                {rxn.emoji} {rxn.count}
                                <span className="reaction-tooltip">{rxn.users.join(', ')}</span>
                              </button>
                            );
                          })}
                        </div>
                      )}

                      {/* Read receipts */}
                      {msg.readBy.length > 0 && (
                        <div className="read-receipts">
                          Seen by {msg.readBy.filter((r) => r.userId !== msg.userId).map((r) => r.userName).join(', ')}
                        </div>
                      )}

                      {/* Message actions */}
                      <div className="message-actions">
                        <button
                          className="action-btn"
                          title="React"
                          onClick={(e) => { e.stopPropagation(); setShowEmojiFor(showEmojiFor === msg.id ? null : msg.id); }}
                        >
                          😊
                        </button>
                        {isOwn && (
                          <button className="action-btn" title="Edit" onClick={() => handleEditMessage(msg)}>
                            ✏️
                          </button>
                        )}
                        {amAdmin && !isOwn && (
                          <>
                            <button className="action-btn btn-kick" title="Kick user" onClick={() => handleKick(msg.userId)}>
                              👢
                            </button>
                          </>
                        )}
                      </div>

                      {/* Emoji picker */}
                      {showEmojiFor === msg.id && (
                        <div className="emoji-picker" onClick={(e) => e.stopPropagation()}>
                          {EMOJIS.map((em) => (
                            <button key={em} className="emoji-option" onClick={() => handleReaction(msg.id, em)}>
                              {em}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
                <div ref={messagesEndRef} />
              </div>

              {/* Typing indicator */}
              <div className="typing-indicator">
                {typingUsers.length === 1 && `${typingUsers[0]} is typing...`}
                {typingUsers.length === 2 && `${typingUsers[0]} and ${typingUsers[1]} are typing...`}
                {typingUsers.length > 2 && 'Multiple users are typing...'}
              </div>

              {/* Input area */}
              <div className="input-area">
                <div className="input-row">
                  <textarea
                    className="message-input"
                    placeholder={showSchedule ? 'Message to schedule...' : 'Type a message...'}
                    value={messageInput}
                    onChange={(e) => handleInputChange(e.target.value)}
                    onKeyDown={handleKeyDown}
                    rows={1}
                  />
                  <button className="btn-small" onClick={handleSendMessage}>
                    {showSchedule ? 'Schedule' : 'Send'}
                  </button>
                </div>
                <div className="input-options">
                  <label>
                    ⏱ Disappears in:
                    <select
                      value={ephemeralSeconds}
                      onChange={(e) => setEphemeralSeconds(parseInt(e.target.value))}
                    >
                      <option value={0}>Never</option>
                      <option value={60}>1 min</option>
                      <option value={300}>5 min</option>
                      <option value={3600}>1 hour</option>
                    </select>
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={showSchedule}
                      onChange={(e) => setShowSchedule(e.target.checked)}
                    />
                    Schedule
                  </label>
                  {showSchedule && (
                    <input
                      type="datetime-local"
                      value={scheduleTime}
                      onChange={(e) => setScheduleTime(e.target.value)}
                      min={new Date().toISOString().slice(0, 16)}
                    />
                  )}
                </div>
              </div>
            </>
          ) : (
            <div className="no-room">
              Select or create a room to start chatting
            </div>
          )}
        </div>

        {/* Members panel */}
        {currentRoom && (
          <aside className="members-panel">
            <h3>Members</h3>
            {members.filter((m) => !m.isBanned).map((m) => (
              <div key={m.userId} className="member-item">
                <div className="member-name-row">
                  <span className={`status-dot ${m.isOnline ? m.status : 'offline'}`} />
                  <span>{m.name}</span>
                  {m.isAdmin && <span className="member-badge">Admin</span>}
                </div>
                {!m.isOnline && m.lastActive && (
                  <div className="user-last-active">Last active {timeAgo(m.lastActive)}</div>
                )}
                {amAdmin && m.userId !== currentUser.id && (
                  <div className="admin-actions">
                    <button className="btn-kick" onClick={() => handleKick(m.userId)}>Kick</button>
                    <button className="btn-ban" onClick={() => handleBan(m.userId)}>Ban</button>
                    {!m.isAdmin && (
                      <button className="btn-promote" onClick={() => handlePromote(m.userId)}>Promote</button>
                    )}
                  </div>
                )}
              </div>
            ))}
            {members.filter((m) => m.isBanned).length > 0 && (
              <>
                <h3 style={{ marginTop: '12px' }}>Banned</h3>
                {members.filter((m) => m.isBanned).map((m) => (
                  <div key={m.userId} className="member-item">
                    <div className="member-name-row">
                      <span className="status-dot offline" />
                      <span style={{ color: 'var(--danger)' }}>{m.name}</span>
                      <span className="member-badge banned">Banned</span>
                    </div>
                  </div>
                ))}
              </>
            )}
          </aside>
        )}
      </div>

      {/* Edit message modal */}
      {editingMessage && (
        <div className="modal-overlay" onClick={() => setEditingMessage(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Edit Message</h3>
            <textarea
              value={editContent}
              onChange={(e) => setEditContent(e.target.value)}
              autoFocus
            />
            <div className="modal-buttons">
              <button className="btn-secondary" onClick={() => setEditingMessage(null)}>Cancel</button>
              <button className="btn-primary" onClick={handleSubmitEdit}>Save</button>
            </div>
          </div>
        </div>
      )}

      {/* Edit history modal */}
      {showHistory && (
        <div className="modal-overlay" onClick={() => setShowHistory(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Edit History</h3>
            <div className="edit-history">
              {showHistory.edits.length === 0 && (
                <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>No edit history available.</p>
              )}
              {showHistory.edits.map((edit, i) => (
                <div key={i} className="history-item">
                  <div className="history-time">{new Date(edit.editedAt).toLocaleString()}</div>
                  <div>{edit.content}</div>
                </div>
              ))}
            </div>
            <div className="modal-buttons">
              <button className="btn-secondary" onClick={() => setShowHistory(null)}>Close</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
