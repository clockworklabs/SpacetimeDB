import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ─── Types ────────────────────────────────────────────────────────────────

interface User {
  id: string;
  username: string;
  status: 'online' | 'away' | 'dnd' | 'invisible';
  lastActive: string;
}

interface Room {
  id: string;
  name: string;
  creatorId: string;
  createdAt: string;
}

interface Message {
  id: string;
  roomId: string;
  senderId: string;
  senderName: string;
  content: string;
  createdAt: string;
  editedAt: string | null;
  isEphemeral: boolean;
  expiresAt: string | null;
  reactions: Record<string, { count: number; users: string[] }>;
  seenBy: string[];
}

interface Member {
  roomId: string;
  userId: string;
  username: string;
  role: string;
  status: string;
  lastActive: string | null;
  isBanned: boolean;
}

interface ScheduledMessage {
  id: string;
  roomId: string;
  content: string;
  scheduledAt: string;
}

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];
const STATUS_COLORS: Record<string, string> = {
  online: '#27ae60',
  away: '#f26522',
  dnd: '#cc3b03',
  invisible: '#848484',
};

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins} minute${mins > 1 ? 's' : ''} ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hour${hrs > 1 ? 's' : ''} ago`;
  return `${Math.floor(hrs / 24)} day${Math.floor(hrs / 24) > 1 ? 's' : ''} ago`;
}

// ─── App ─────────────────────────────────────────────────────────────────

export default function App() {
  const [socket, setSocket] = useState<Socket | null>(null);
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoom, setCurrentRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [messageInput, setMessageInput] = useState('');
  const [allUsers, setAllUsers] = useState<User[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [typingUsers, setTypingUsers] = useState<Map<string, string>>(new Map());
  const [unreadCounts, setUnreadCounts] = useState<Record<string, number>>({});
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [ephemeralMode, setEphemeralMode] = useState<string>('none');
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [showScheduledList, setShowScheduledList] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');
  const [historyMessage, setHistoryMessage] = useState<{ id: string; history: { content: string; editedAt: string }[] } | null>(null);
  const [showMembers, setShowMembers] = useState(false);
  const [userStatus, setUserStatus] = useState<'online' | 'away' | 'dnd' | 'invisible'>('online');

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastReadRef = useRef<Record<string, Set<string>>>({});

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Initialize socket
  useEffect(() => {
    const s = io({ path: '/socket.io' });
    setSocket(s);

    s.on('room:new', (room: Room) => {
      setRooms(prev => [room, ...prev]);
    });

    s.on('user:status', (data: { userId: string; status: string; username?: string; lastActive: string }) => {
      setAllUsers(prev => prev.map(u =>
        u.id === data.userId ? { ...u, status: data.status as User['status'], lastActive: data.lastActive } : u
      ));
    });

    s.on('room:kicked', ({ roomId }: { roomId: string }) => {
      setCurrentRoom(prev => {
        if (prev?.id === roomId) return null;
        return prev;
      });
      setMessages([]);
      alert('You have been kicked from this room');
    });

    s.on('scheduled:created', (sm: ScheduledMessage) => {
      setScheduledMessages(prev => [...prev, sm]);
    });

    s.on('scheduled:cancelled', ({ scheduledId }: { scheduledId: string }) => {
      setScheduledMessages(prev => prev.filter(s => s.id !== scheduledId));
    });

    return () => { s.disconnect(); };
  }, []);

  // Room-specific socket events
  useEffect(() => {
    if (!socket || !currentUser) return;

    const handleNewMessage = (msg: Message) => {
      if (msg.roomId !== currentRoom?.id) {
        setUnreadCounts(prev => ({ ...prev, [msg.roomId]: (prev[msg.roomId] || 0) + 1 }));
        return;
      }
      setMessages(prev => {
        if (prev.some(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
    };

    const handleMessageDeleted = ({ messageId }: { messageId: string }) => {
      setMessages(prev => prev.filter(m => m.id !== messageId));
    };

    const handleMessageEdited = (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? msg : m));
    };

    const handleReactionsUpdated = (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? msg : m));
    };

    const handleReadReceipt = (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? msg : m));
    };

    const handleTyping = ({ roomId, userId, username, isTyping }: { roomId: string; userId: string; username: string; isTyping: boolean }) => {
      if (roomId !== currentRoom?.id) return;
      if (userId === currentUser.id) return;
      setTypingUsers(prev => {
        const next = new Map(prev);
        if (isTyping) next.set(userId, username);
        else next.delete(userId);
        return next;
      });
    };

    const handleMemberJoined = ({ roomId, userId, username }: { roomId: string; userId: string; username: string }) => {
      if (roomId !== currentRoom?.id) return;
      setMembers(prev => {
        if (prev.some(m => m.userId === userId)) return prev;
        return [...prev, { roomId, userId, username, role: 'member', status: 'online', lastActive: null, isBanned: false }];
      });
    };

    const handleMemberLeft = ({ roomId, userId }: { roomId: string; userId: string }) => {
      if (roomId !== currentRoom?.id) return;
      setMembers(prev => prev.filter(m => m.userId !== userId));
    };

    const handleMemberKicked = ({ roomId, userId }: { roomId: string; userId: string }) => {
      if (roomId !== currentRoom?.id) return;
      setMembers(prev => prev.filter(m => m.userId !== userId));
    };

    const handleMemberPromoted = ({ roomId, userId }: { roomId: string; userId: string }) => {
      if (roomId !== currentRoom?.id) return;
      setMembers(prev => prev.map(m => m.userId === userId ? { ...m, role: 'admin' } : m));
    };

    socket.on('message:new', handleNewMessage);
    socket.on('message:deleted', handleMessageDeleted);
    socket.on('message:edited', handleMessageEdited);
    socket.on('message:reactions-updated', handleReactionsUpdated);
    socket.on('message:read-receipt', handleReadReceipt);
    socket.on('typing:update', handleTyping);
    socket.on('room:member-joined', handleMemberJoined);
    socket.on('room:member-left', handleMemberLeft);
    socket.on('room:member-kicked', handleMemberKicked);
    socket.on('room:member-promoted', handleMemberPromoted);

    return () => {
      socket.off('message:new', handleNewMessage);
      socket.off('message:deleted', handleMessageDeleted);
      socket.off('message:edited', handleMessageEdited);
      socket.off('message:reactions-updated', handleReactionsUpdated);
      socket.off('message:read-receipt', handleReadReceipt);
      socket.off('typing:update', handleTyping);
      socket.off('room:member-joined', handleMemberJoined);
      socket.off('room:member-left', handleMemberLeft);
      socket.off('room:member-kicked', handleMemberKicked);
      socket.off('room:member-promoted', handleMemberPromoted);
    };
  }, [socket, currentUser, currentRoom?.id]);

  // Mark messages as read
  const markMessagesRead = useCallback((msgs: Message[], userId: string, roomId: string) => {
    if (!socket) return;
    if (!lastReadRef.current[roomId]) lastReadRef.current[roomId] = new Set();
    const toMark = msgs
      .filter(m => m.senderId !== userId && !lastReadRef.current[roomId].has(m.id))
      .map(m => m.id);
    if (toMark.length > 0) {
      toMark.forEach(id => lastReadRef.current[roomId].add(id));
      socket.emit('message:read', { roomId, userId, messageIds: toMark });
    }
  }, [socket]);

  useEffect(() => {
    if (currentUser && messages.length > 0 && currentRoom) {
      markMessagesRead(messages, currentUser.id, currentRoom.id);
    }
  }, [messages, currentUser, currentRoom, markMessagesRead]);

  const handleRegister = async () => {
    const name = nameInput.trim();
    if (!name) return;
    try {
      const res = await fetch('/api/users', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: name }),
      });
      const user = await res.json() as User;
      setCurrentUser(user);
      setUserStatus(user.status);
      socket?.emit('user:connect', { userId: user.id });

      const [roomsRes, usersRes, schedRes] = await Promise.all([
        fetch('/api/rooms'),
        fetch('/api/users/online'),
        fetch(`/api/users/${user.id}/scheduled`),
      ]);
      setRooms(await roomsRes.json());
      setAllUsers(await usersRes.json());
      setScheduledMessages(await schedRes.json());
    } catch (e) {
      console.error(e);
    }
  };

  const handleJoinRoom = async (room: Room) => {
    if (!currentUser || !socket) return;
    if (currentRoom) {
      socket.emit('room:leave', { roomId: currentRoom.id, userId: currentUser.id });
    }
    setCurrentRoom(room);
    setMessages([]);
    setTypingUsers(new Map());
    setShowMembers(false);
    setUnreadCounts(prev => ({ ...prev, [room.id]: 0 }));

    socket.emit('room:join', { roomId: room.id, userId: currentUser.id });

    const [msgsRes, membersRes] = await Promise.all([
      fetch(`/api/rooms/${room.id}/messages`),
      fetch(`/api/rooms/${room.id}/members`),
    ]);
    const msgs: Message[] = await msgsRes.json();
    setMessages(msgs);
    setMembers(await membersRes.json());
    markMessagesRead(msgs, currentUser.id, room.id);
  };

  const handleLeaveRoom = () => {
    if (!currentRoom || !currentUser || !socket) return;
    socket.emit('room:leave', { roomId: currentRoom.id, userId: currentUser.id });
    setCurrentRoom(null);
    setMessages([]);
    setMembers([]);
    setTypingUsers(new Map());
  };

  const handleCreateRoom = async () => {
    const name = newRoomName.trim();
    if (!name || !currentUser) return;
    try {
      await fetch('/api/rooms', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, creatorId: currentUser.id }),
      });
      setNewRoomName('');
      setShowCreateRoom(false);
    } catch (e) {
      console.error(e);
    }
  };

  const handleSendMessage = () => {
    if (!messageInput.trim() || !currentRoom || !currentUser || !socket) return;

    if (showSchedule && scheduleTime) {
      socket.emit('message:schedule', {
        roomId: currentRoom.id,
        userId: currentUser.id,
        content: messageInput.trim(),
        scheduledAt: new Date(scheduleTime).toISOString(),
      });
      setMessageInput('');
      setShowSchedule(false);
      setScheduleTime('');
      return;
    }

    const duration = ephemeralMode !== 'none' ? parseInt(ephemeralMode) : undefined;
    socket.emit('message:send', {
      roomId: currentRoom.id,
      userId: currentUser.id,
      content: messageInput.trim(),
      isEphemeral: ephemeralMode !== 'none',
      ephemeralDuration: duration,
    });
    setMessageInput('');

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    socket.emit('typing:stop', { roomId: currentRoom.id, userId: currentUser.id, username: currentUser.username });
  };

  const handleTyping = (value: string) => {
    setMessageInput(value);
    if (!currentRoom || !currentUser || !socket) return;

    socket.emit('typing:start', { roomId: currentRoom.id, userId: currentUser.id, username: currentUser.username });
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socket.emit('typing:stop', { roomId: currentRoom.id, userId: currentUser.id, username: currentUser.username });
    }, 3000);
  };

  const handleReact = (messageId: string, emoji: string) => {
    if (!currentUser || !socket) return;
    socket.emit('message:react', { messageId, userId: currentUser.id, emoji });
  };

  const handleEditSave = (messageId: string) => {
    if (!currentUser || !socket || !editContent.trim()) return;
    socket.emit('message:edit', { messageId, userId: currentUser.id, newContent: editContent.trim() });
    setEditingMessageId(null);
    setEditContent('');
  };

  const handleShowHistory = async (messageId: string) => {
    const res = await fetch(`/api/messages/${messageId}/history`);
    const hist = await res.json() as { content: string; editedAt: string }[];
    setHistoryMessage({ id: messageId, history: hist });
  };

  const handleKick = (targetUserId: string) => {
    if (!currentRoom || !currentUser || !socket) return;
    socket.emit('room:kick', { roomId: currentRoom.id, targetUserId, adminId: currentUser.id });
  };

  const handlePromote = (targetUserId: string) => {
    if (!currentRoom || !currentUser || !socket) return;
    socket.emit('room:promote', { roomId: currentRoom.id, targetUserId, adminId: currentUser.id });
  };

  const handleCancelScheduled = (scheduledId: string) => {
    if (!currentUser || !socket) return;
    socket.emit('message:cancel-scheduled', { scheduledId, userId: currentUser.id });
  };

  const handleSetStatus = (status: 'online' | 'away' | 'dnd' | 'invisible') => {
    if (!currentUser || !socket) return;
    setUserStatus(status);
    socket.emit('user:set-status', { userId: currentUser.id, status });
  };

  const myMembership = currentRoom ? members.find(m => m.userId === currentUser?.id) : null;
  const isAdmin = myMembership?.role === 'admin';

  // Countdown for ephemeral messages
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  function ephemeralCountdown(expiresAt: string): string {
    const remaining = Math.max(0, Math.floor((new Date(expiresAt).getTime() - now) / 1000));
    if (remaining === 0) return 'expiring...';
    if (remaining < 60) return `expires in ${remaining}s`;
    return `expires in ${Math.ceil(remaining / 60)}m`;
  }

  // ─── Login screen ───────────────────────────────────────────────────────

  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>PostgreSQL Chat</h1>
          <p className="subtitle">Real-time chat powered by PostgreSQL</p>
          <div className="login-form">
            <input
              type="text"
              placeholder="Enter your name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleRegister()}
              maxLength={30}
            />
            <button type="submit" onClick={handleRegister}>Join</button>
          </div>
        </div>
      </div>
    );
  }

  // ─── Main app ───────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* Header */}
      <header className="app-header">
        <h1>PostgreSQL Chat</h1>
        <div className="header-right">
          <span className="current-user">
            <span className="status-dot" style={{ background: STATUS_COLORS[userStatus] }} />
            {currentUser.username}
          </span>
          <select
            value={userStatus}
            onChange={e => handleSetStatus(e.target.value as typeof userStatus)}
            className="status-select"
            aria-label="Set status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>
      </header>

      <div className="app-body">
        {/* Sidebar */}
        <aside className="sidebar">
          <div className="sidebar-section">
            <div className="sidebar-title">
              Rooms
              <button onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
            </div>
            {showCreateRoom && (
              <div className="create-room-form">
                <input
                  type="text"
                  placeholder="Room name"
                  value={newRoomName}
                  onChange={e => setNewRoomName(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                  maxLength={50}
                />
                <button onClick={handleCreateRoom}>Create</button>
                <button onClick={() => setShowCreateRoom(false)} className="btn-secondary">Cancel</button>
              </div>
            )}
            <ul className="room-list">
              {rooms.map(room => (
                <li
                  key={room.id}
                  className={`room-item ${currentRoom?.id === room.id ? 'active' : ''}`}
                  onClick={() => handleJoinRoom(room)}
                >
                  <span>{room.name}</span>
                  {unreadCounts[room.id] > 0 && (
                    <span className="badge">{unreadCounts[room.id]}</span>
                  )}
                </li>
              ))}
            </ul>
          </div>

          <div className="sidebar-section online-users">
            <div className="sidebar-title">Users</div>
            <ul className="user-list">
              {allUsers.map(u => (
                <li key={u.id} className="user-item">
                  <span
                    className="status-dot"
                    style={{ background: STATUS_COLORS[u.status] }}
                    title={u.status}
                  />
                  <span className="username">{u.username}</span>
                  {(u.status === 'away' || u.status === 'invisible') && u.lastActive && (
                    <span className="last-active" title={u.lastActive}>
                      Last active {timeAgo(u.lastActive)}
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>

          {scheduledMessages.length > 0 && (
            <div className="sidebar-section">
              <div className="sidebar-title">
                <button
                  onClick={() => setShowScheduledList(!showScheduledList)}
                  className="btn-text"
                >
                  Scheduled ({scheduledMessages.length})
                </button>
              </div>
              {showScheduledList && (
                <ul className="scheduled-list">
                  {scheduledMessages.map(sm => (
                    <li key={sm.id} className="scheduled-item">
                      <div className="scheduled-content">
                        <span className="scheduled-text">{sm.content}</span>
                        <span className="scheduled-time">
                          {new Date(sm.scheduledAt).toLocaleString()}
                        </span>
                      </div>
                      <button onClick={() => handleCancelScheduled(sm.id)} className="btn-danger btn-sm">
                        Cancel
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </aside>

        {/* Main chat area */}
        <main className="chat-area">
          {!currentRoom ? (
            <div className="no-room">
              <p>Select a room or create a new one to start chatting</p>
            </div>
          ) : (
            <>
              {/* Room header */}
              <div className="room-header">
                <div className="room-title">
                  <h2>{currentRoom.name}</h2>
                </div>
                <div className="room-actions">
                  <button onClick={() => setShowMembers(!showMembers)}>Members</button>
                  <button onClick={handleLeaveRoom} className="btn-secondary">Leave</button>
                </div>
              </div>

              <div className="chat-content">
                {/* Messages */}
                <div className="messages">
                  {messages.map(msg => (
                    <div key={msg.id} className={`message ${msg.senderId === currentUser.id ? 'own' : ''}`}>
                      <div className="message-header">
                        <span className="message-sender">{msg.senderName}</span>
                        <span className="message-time">
                          {new Date(msg.createdAt).toLocaleTimeString()}
                        </span>
                      </div>

                      {editingMessageId === msg.id ? (
                        <div className="edit-form">
                          <input
                            type="text"
                            value={editContent}
                            onChange={e => setEditContent(e.target.value)}
                            onKeyDown={e => {
                              if (e.key === 'Enter') handleEditSave(msg.id);
                              if (e.key === 'Escape') setEditingMessageId(null);
                            }}
                            autoFocus
                          />
                          <button onClick={() => handleEditSave(msg.id)}>Save</button>
                          <button onClick={() => setEditingMessageId(null)} className="btn-secondary">Cancel</button>
                        </div>
                      ) : (
                        <div className="message-content">
                          <span>{msg.content}</span>
                          {msg.editedAt && (
                            <span
                              className="edited-indicator"
                              onClick={() => handleShowHistory(msg.id)}
                              style={{ cursor: 'pointer' }}
                              title="View edit history"
                            >
                              (edited)
                            </span>
                          )}
                          {msg.isEphemeral && msg.expiresAt && (
                            <span className="ephemeral-indicator">
                              {ephemeralCountdown(msg.expiresAt)}
                            </span>
                          )}
                        </div>
                      )}

                      {/* Reactions */}
                      <div className="message-reactions">
                        {Object.entries(msg.reactions).map(([emoji, data]) => (
                          <button
                            key={emoji}
                            className="reaction-btn"
                            onClick={() => handleReact(msg.id, emoji)}
                            title={data.users.join(', ')}
                          >
                            {emoji} {data.count}
                          </button>
                        ))}
                        <div className="reaction-picker">
                          {EMOJIS.map(emoji => (
                            <button
                              key={emoji}
                              className="emoji-btn"
                              onClick={() => handleReact(msg.id, emoji)}
                              aria-label={`React with ${emoji}`}
                            >
                              {emoji}
                            </button>
                          ))}
                        </div>
                      </div>

                      {/* Read receipts */}
                      {msg.seenBy.length > 0 && (
                        <div className="read-receipt">
                          Seen by {msg.seenBy.join(', ')}
                        </div>
                      )}

                      {/* Message actions */}
                      {msg.senderId === currentUser.id && !editingMessageId && (
                        <div className="message-actions">
                          <button
                            onClick={() => { setEditingMessageId(msg.id); setEditContent(msg.content); }}
                          >
                            Edit
                          </button>
                        </div>
                      )}
                    </div>
                  ))}
                  <div ref={messagesEndRef} />
                </div>

                {/* Typing indicator */}
                {typingUsers.size > 0 && (
                  <div className="typing-indicator">
                    {typingUsers.size === 1
                      ? `${[...typingUsers.values()][0]} is typing...`
                      : `Multiple users are typing...`}
                  </div>
                )}

                {/* Input area */}
                <div className="input-area">
                  <div className="input-controls">
                    <select
                      value={ephemeralMode}
                      onChange={e => setEphemeralMode(e.target.value)}
                      className="ephemeral-select"
                      aria-label="ephemeral"
                    >
                      <option value="none">Normal</option>
                      <option value="30">Disappear 30s</option>
                      <option value="60">Disappear 1m</option>
                      <option value="300">Disappear 5m</option>
                    </select>
                    <button
                      onClick={() => setShowSchedule(!showSchedule)}
                      className={`btn-schedule ${showSchedule ? 'active' : ''}`}
                      title="Schedule message"
                      aria-label="Schedule"
                    >
                      Schedule
                    </button>
                  </div>
                  {showSchedule && (
                    <div className="schedule-picker">
                      <input
                        type="datetime-local"
                        value={scheduleTime}
                        onChange={e => setScheduleTime(e.target.value)}
                      />
                    </div>
                  )}
                  <div className="message-input-row">
                    <input
                      type="text"
                      placeholder="Type a message..."
                      value={messageInput}
                      onChange={e => handleTyping(e.target.value)}
                      onKeyDown={e => e.key === 'Enter' && handleSendMessage()}
                      className="message-input"
                      maxLength={2000}
                    />
                    <button onClick={handleSendMessage} className="btn-send">
                      {showSchedule ? 'Schedule' : 'Send'}
                    </button>
                  </div>
                </div>
              </div>
            </>
          )}
        </main>

        {/* Members panel */}
        {showMembers && currentRoom && (
          <aside className="members-panel">
            <h3>Members</h3>
            <ul>
              {members.map(m => (
                <li key={m.userId} className="member-item">
                  <span className="status-dot" style={{ background: STATUS_COLORS[m.status] || '#848484' }} />
                  <span className="member-name">{m.username}</span>
                  {m.role === 'admin' && <span className="admin-badge">Admin</span>}
                  {isAdmin && m.userId !== currentUser.id && (
                    <div className="member-actions">
                      <button onClick={() => handleKick(m.userId)} className="btn-danger btn-sm">Kick</button>
                      {m.role !== 'admin' && (
                        <button onClick={() => handlePromote(m.userId)} className="btn-sm">Promote</button>
                      )}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </aside>
        )}
      </div>

      {/* Edit history modal */}
      {historyMessage && (
        <div className="modal-overlay" onClick={() => setHistoryMessage(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            {historyMessage.history.length === 0 ? (
              <p>No edit history available</p>
            ) : (
              <ul className="history-list">
                {historyMessage.history.map((h, i) => (
                  <li key={i}>
                    <span className="history-time">{new Date(h.editedAt).toLocaleString()}</span>
                    <span className="history-content">{h.content}</span>
                  </li>
                ))}
              </ul>
            )}
            <button onClick={() => setHistoryMessage(null)}>Close</button>
          </div>
        </div>
      )}
    </div>
  );
}
