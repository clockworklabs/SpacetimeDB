import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ── Types ─────────────────────────────────────────────────────────────────────
interface User { id: number; name: string; }

interface Room {
  id: number;
  name: string;
  createdBy: number;
  memberIds: number[];
  unreadCount: number;
}

interface Reaction {
  emoji: string;
  userIds: number[];
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  createdAt: string;
  readBy: number[];
  expiresAt?: string | null;
  reactions: Reaction[];
  isEdited: boolean;
  editedAt?: string | null;
}

interface MessageEdit {
  id: number;
  messageId: number;
  userId: number;
  previousContent: string;
  editedAt: string;
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  scheduledFor: string;
  status: string;
}

// ── Helpers ───────────────────────────────────────────────────────────────────
function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function typingLabel(names: string[], currentUserId: number, typingUserIds: number[]): string {
  const others = typingUserIds.filter(id => id !== currentUserId);
  if (others.length === 0) return '';
  const otherNames = others.map(id => {
    const idx = typingUserIds.indexOf(id);
    return names[idx] ?? 'Someone';
  });
  // names array is parallel to typingUserIds
  const filtered = typingUserIds
    .filter(id => id !== currentUserId)
    .map(id => {
      const idx = typingUserIds.indexOf(id);
      return names[idx] ?? 'Someone';
    });
  if (filtered.length === 1) return `${filtered[0]} is typing...`;
  if (filtered.length === 2) return `${filtered[0]} and ${filtered[1]} are typing...`;
  return 'Multiple users are typing...';
}

// ── App ───────────────────────────────────────────────────────────────────────
export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(() => {
    const stored = localStorage.getItem('chat_user');
    return stored ? JSON.parse(stored) : null;
  });
  const [nameInput, setNameInput] = useState('');
  const [loginError, setLoginError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<number | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [allUsers, setAllUsers] = useState<User[]>([]);
  const [onlineUserIds, setOnlineUserIds] = useState<number[]>([]);

  const [typingNames, setTypingNames] = useState<string[]>([]);
  const [typingUserIds, setTypingUserIds] = useState<number[]>([]);

  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [scheduleMode, setScheduleMode] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [expiresAfterSeconds, setExpiresAfterSeconds] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const [newRoomName, setNewRoomName] = useState('');
  const [messageInput, setMessageInput] = useState('');

  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editInput, setEditInput] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);

  const socketRef = useRef<Socket | null>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const isTypingRef = useRef(false);

  // ── Helpers ─────────────────────────────────────────────────────────────────
  const getUserName = useCallback((userId: number) => {
    return allUsers.find(u => u.id === userId)?.name ?? `User ${userId}`;
  }, [allUsers]);

  const currentRoom = rooms.find(r => r.id === currentRoomId) ?? null;
  const isMember = currentRoom?.memberIds.includes(currentUser?.id ?? -1) ?? false;

  // ── Socket setup ─────────────────────────────────────────────────────────────
  useEffect(() => {
    if (!currentUser) return;

    const socket = io({ path: '/socket.io' });
    socketRef.current = socket;

    socket.on('connect', () => {
      socket.emit('user:online', { userId: currentUser.id });
    });

    socket.on('users:online', (ids: number[]) => {
      setOnlineUserIds(ids);
    });

    socket.on('message:new', (msg: Message) => {
      setMessages(prev => {
        if (prev.find(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      // If we're in this room, mark as read
      setCurrentRoomId(curr => {
        if (curr === msg.roomId) {
          // mark read via REST (debounce to avoid per-message calls)
          fetch(`/api/rooms/${msg.roomId}/read`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ userId: currentUser.id }),
          });
        }
        return curr;
      });
    });

    socket.on('reads:update', ({ messageId, userId }: { messageId: number; userId: number }) => {
      setMessages(prev => prev.map(m =>
        m.id === messageId && !m.readBy.includes(userId)
          ? { ...m, readBy: [...m.readBy, userId] }
          : m
      ));
    });

    socket.on('unread:update', ({ roomId, count }: { roomId: number; count: number }) => {
      setRooms(prev => prev.map(r => r.id === roomId ? { ...r, unreadCount: count } : r));
    });

    socket.on('room:created', (room: Room) => {
      setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
    });

    socket.on('room:membership', ({ roomId, userId, action }: { roomId: number; userId: number; action: 'join' | 'leave' }) => {
      setRooms(prev => prev.map(r => {
        if (r.id !== roomId) return r;
        const memberIds = action === 'join'
          ? (r.memberIds.includes(userId) ? r.memberIds : [...r.memberIds, userId])
          : r.memberIds.filter(id => id !== userId);
        return { ...r, memberIds };
      }));
    });

    socket.on('user:registered', (user: { id: number; name: string }) => {
      setAllUsers(prev => prev.find(u => u.id === user.id) ? prev : [...prev, user]);
    });

    socket.on('scheduled:sent', ({ id }: { id: number }) => {
      setScheduledMessages(prev => prev.filter(m => m.id !== id));
    });

    socket.on('message:deleted', ({ messageId }: { messageId: number }) => {
      setMessages(prev => prev.filter(m => m.id !== messageId));
    });

    socket.on('reaction:update', ({ messageId, reactions }: { messageId: number; reactions: Reaction[] }) => {
      setMessages(prev => prev.map(m => m.id === messageId ? { ...m, reactions } : m));
    });

    socket.on('message:edited', ({ messageId, content, editedAt }: { messageId: number; content: string; editedAt: string }) => {
      setMessages(prev => prev.map(m =>
        m.id === messageId ? { ...m, content, isEdited: true, editedAt } : m
      ));
    });

    socket.on('typing:update', ({ roomId, typingUserIds: ids, typingNames: names }: { roomId: number; typingUserIds: number[]; typingNames: string[] }) => {
      setCurrentRoomId(curr => {
        if (curr === roomId) {
          setTypingUserIds(ids);
          setTypingNames(names);
        }
        return curr;
      });
    });

    return () => {
      socket.disconnect();
      socketRef.current = null;
    };
  }, [currentUser]);

  // ── Tick every second for ephemeral countdowns ────────────────────────────────
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  // ── Subscribe to room socket channel when entering ────────────────────────────
  useEffect(() => {
    const socket = socketRef.current;
    if (!socket || !currentRoomId) return;
    socket.emit('room:subscribe', { roomId: currentRoomId });
    setTypingUserIds([]);
    setTypingNames([]);
    return () => {
      socket.emit('room:unsubscribe', { roomId: currentRoomId });
    };
  }, [currentRoomId]);

  // ── Load initial data ─────────────────────────────────────────────────────────
  useEffect(() => {
    if (!currentUser) return;
    fetch('/api/users').then(r => r.json()).then(setAllUsers);
    fetch(`/api/rooms?userId=${currentUser.id}`).then(r => r.json()).then(setRooms);
    fetch('/api/users/online').then(r => r.json()).then(setOnlineUserIds);
    fetch(`/api/users/${currentUser.id}/scheduled`).then(r => r.json()).then(setScheduledMessages);
  }, [currentUser]);

  // ── Load messages when switching rooms ────────────────────────────────────────
  useEffect(() => {
    if (!currentRoomId || !currentUser) return;
    setMessages([]);
    fetch(`/api/rooms/${currentRoomId}/messages`)
      .then(r => r.json())
      .then(msgs => {
        setMessages(msgs);
        // Mark room as read
        fetch(`/api/rooms/${currentRoomId}/read`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ userId: currentUser.id }),
        });
      });
  }, [currentRoomId, currentUser]);

  // ── Scroll to bottom on new messages ────────────────────────────────────────
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // ── Login ─────────────────────────────────────────────────────────────────────
  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    const name = nameInput.trim();
    if (!name) return;
    setLoginError('');
    try {
      const res = await fetch('/api/users', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      });
      if (!res.ok) {
        const err = await res.json();
        setLoginError(err.error ?? 'Failed to set name');
        return;
      }
      const user: User = await res.json();
      localStorage.setItem('chat_user', JSON.stringify(user));
      setCurrentUser(user);
    } catch {
      setLoginError('Connection error');
    }
  }

  // ── Room actions ──────────────────────────────────────────────────────────────
  async function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    if (!newRoomName.trim() || !currentUser) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    if (res.ok) {
      const room: Room = await res.json();
      setNewRoomName('');
      setCurrentRoomId(room.id);
    }
  }

  async function handleJoinRoom() {
    if (!currentRoomId || !currentUser) return;
    await fetch(`/api/rooms/${currentRoomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
  }

  async function handleLeaveRoom() {
    if (!currentRoomId || !currentUser) return;
    await fetch(`/api/rooms/${currentRoomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setCurrentRoomId(null);
  }

  function handleSelectRoom(roomId: number) {
    setCurrentRoomId(roomId);
  }

  // ── Typing ────────────────────────────────────────────────────────────────────
  function handleTyping() {
    if (!currentUser || !currentRoomId) return;
    const socket = socketRef.current;
    if (!socket) return;

    if (!isTypingRef.current) {
      isTypingRef.current = true;
      socket.emit('typing:start', { roomId: currentRoomId, userId: currentUser.id });
    }

    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      isTypingRef.current = false;
      socket.emit('typing:stop', { roomId: currentRoomId, userId: currentUser.id });
    }, 2000);
  }

  // ── Send message ──────────────────────────────────────────────────────────────
  async function handleSend(e: React.FormEvent) {
    e.preventDefault();
    if (!messageInput.trim() || !currentRoomId || !currentUser) return;

    // Stop typing
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    isTypingRef.current = false;
    socketRef.current?.emit('typing:stop', { roomId: currentRoomId, userId: currentUser.id });

    const content = messageInput.trim();
    setMessageInput('');

    const body: Record<string, unknown> = { userId: currentUser.id, content };
    if (expiresAfterSeconds !== null) body.expiresAfterSeconds = expiresAfterSeconds;

    const res = await fetch(`/api/rooms/${currentRoomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      setMessageInput(content); // restore on failure
    }
  }

  // ── Schedule message ──────────────────────────────────────────────────────────
  async function handleSchedule(e: React.FormEvent) {
    e.preventDefault();
    if (!messageInput.trim() || !currentRoomId || !currentUser || !scheduleTime) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/scheduled`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: messageInput.trim(), scheduledFor: scheduleTime }),
    });
    if (res.ok) {
      const msg: ScheduledMessage = await res.json();
      setScheduledMessages(prev => [...prev, msg]);
      setMessageInput('');
      setScheduleMode(false);
      setScheduleTime('');
    }
  }

  async function handleCancelScheduled(id: number) {
    if (!currentUser) return;
    const res = await fetch(`/api/scheduled/${id}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    if (res.ok) {
      setScheduledMessages(prev => prev.filter(m => m.id !== id));
    }
  }

  function getMinScheduleTime(): string {
    const d = new Date(Date.now() + 10000); // 10 seconds from now
    // datetime-local format: YYYY-MM-DDTHH:MM:SS
    return d.toISOString().slice(0, 19);
  }

  // ── Ephemeral countdown ──────────────────────────────────────────────────────
  function getCountdown(expiresAt: string): string {
    const msLeft = new Date(expiresAt).getTime() - now;
    if (msLeft <= 0) return 'expiring...';
    const s = Math.ceil(msLeft / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    const rem = s % 60;
    return rem > 0 ? `${m}m ${rem}s` : `${m}m`;
  }

  // ── Reactions ────────────────────────────────────────────────────────────────
  async function handleReaction(messageId: number, emoji: string) {
    if (!currentUser) return;
    await fetch(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
  }

  // ── Message editing ──────────────────────────────────────────────────────────
  function startEdit(msg: Message) {
    setEditingMessageId(msg.id);
    setEditInput(msg.content);
  }

  function cancelEdit() {
    setEditingMessageId(null);
    setEditInput('');
  }

  async function handleEditSubmit(e: React.FormEvent, messageId: number) {
    e.preventDefault();
    if (!editInput.trim() || !currentUser) return;
    await fetch(`/api/messages/${messageId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: editInput.trim() }),
    });
    setEditingMessageId(null);
    setEditInput('');
  }

  async function handleShowHistory(messageId: number) {
    const [msg, edits] = await Promise.all([
      fetch(`/api/messages/${messageId}/edits`).then(r => r.json()),
      Promise.resolve([]),
    ]);
    // msg is actually the edits array here
    setEditHistory(msg as MessageEdit[]);
    setHistoryMessageId(messageId);
  }

  function closeHistory() {
    setHistoryMessageId(null);
    setEditHistory([]);
  }

  // ── Read receipts display ────────────────────────────────────────────────────
  function getSeenBy(msg: Message): string {
    if (!currentUser) return '';
    const readers = msg.readBy.filter(uid => uid !== msg.userId);
    if (readers.length === 0) return '';
    const names = readers.map(uid => getUserName(uid));
    return `Seen by ${names.join(', ')}`;
  }

  // ── Login screen ──────────────────────────────────────────────────────────────
  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>SpacetimeDB Chat</h1>
          <p>Enter a display name to get started</p>
          <form onSubmit={handleLogin}>
            <input
              type="text"
              placeholder="Your name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              maxLength={32}
              autoFocus
            />
            {loginError && <p style={{ color: 'var(--danger)', fontSize: '0.85rem', marginBottom: 8 }}>{loginError}</p>}
            <button className="btn btn-primary" type="submit" disabled={!nameInput.trim()}>
              Join Chat
            </button>
          </form>
        </div>
      </div>
    );
  }

  // ── Main layout ───────────────────────────────────────────────────────────────
  const typingText = currentUser
    ? typingLabel(typingNames, currentUser.id, typingUserIds)
    : '';

  return (
    <div className="app-layout">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>SpacetimeDB Chat</h2>
        </div>

        <div className="user-badge">
          <div className="online-dot" />
          <div>
            <div><strong>{currentUser.name}</strong></div>
            <span>{onlineUserIds.length} online</span>
          </div>
        </div>

        <div className="section-title">Rooms</div>
        <div className="room-list">
          {rooms.map(room => (
            <div
              key={room.id}
              className={`room-item${currentRoomId === room.id ? ' active' : ''}`}
              onClick={() => handleSelectRoom(room.id)}
            >
              <span className="room-item-name"># {room.name}</span>
              {room.unreadCount > 0 && (
                <span className="unread-badge">{room.unreadCount > 99 ? '99+' : room.unreadCount}</span>
              )}
            </div>
          ))}
          {rooms.length === 0 && (
            <div style={{ padding: '12px 16px', color: 'var(--text-muted)', fontSize: '0.85rem' }}>
              No rooms yet
            </div>
          )}
        </div>

        <div className="create-room">
          <form className="create-room-form" onSubmit={handleCreateRoom}>
            <input
              type="text"
              placeholder="New room name"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              maxLength={64}
            />
            <button className="btn btn-primary btn-sm" type="submit" disabled={!newRoomName.trim()}>+</button>
          </form>
        </div>

        {/* Online users */}
        <div className="online-section">
          <div className="section-title" style={{ padding: '0 0 6px' }}>Online</div>
          {onlineUserIds.length === 0 && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.82rem' }}>Nobody online</div>
          )}
          {onlineUserIds.map(uid => (
            <div key={uid} className="online-user">
              <div className="online-dot" />
              <span>{getUserName(uid)}{uid === currentUser.id ? ' (you)' : ''}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Chat area */}
      <div className="chat-area">
        {!currentRoom ? (
          <div className="empty-state">Select a room to start chatting</div>
        ) : (
          <>
            <div className="chat-header">
              <h2># {currentRoom.name}</h2>
              <span className="member-info">{currentRoom.memberIds.length} members</span>
              {isMember ? (
                <button className="btn btn-ghost btn-sm btn-danger" onClick={handleLeaveRoom}>Leave</button>
              ) : (
                <button className="btn btn-primary btn-sm" onClick={handleJoinRoom}>Join</button>
              )}
            </div>

            <div className="messages-container">
              {messages.length === 0 && (
                <div style={{ color: 'var(--text-muted)', fontSize: '0.9rem', textAlign: 'center', marginTop: 32 }}>
                  No messages yet. Be the first!
                </div>
              )}
              {messages.map(msg => {
                const isMe = msg.userId === currentUser.id;
                const seenBy = getSeenBy(msg);
                const isEphemeral = !!msg.expiresAt;
                const isEditing = editingMessageId === msg.id;
                return (
                  <div key={msg.id} className={`message${isEphemeral ? ' ephemeral-message' : ''}`}>
                    <div className="message-header">
                      <span className={`message-author${isMe ? ' is-me' : ''}`}>{getUserName(msg.userId)}</span>
                      <span className="message-time">{formatTime(msg.createdAt)}</span>
                      {isEphemeral && (
                        <span className="ephemeral-badge" title="This message will disappear">
                          ⏱ {getCountdown(msg.expiresAt!)}
                        </span>
                      )}
                      {msg.isEdited && (
                        <span
                          className="edited-indicator"
                          title={msg.editedAt ? `Edited at ${new Date(msg.editedAt).toLocaleTimeString()}` : 'Edited'}
                          onClick={() => handleShowHistory(msg.id)}
                          style={{ cursor: 'pointer' }}
                        >(edited)</span>
                      )}
                      {isMe && !isEphemeral && isMember && !isEditing && (
                        <button
                          className="btn btn-ghost btn-sm edit-btn"
                          onClick={() => startEdit(msg)}
                          title="Edit message"
                          style={{ marginLeft: 'auto', opacity: 0.6, fontSize: '0.75rem' }}
                        >Edit</button>
                      )}
                    </div>
                    {isEditing ? (
                      <form onSubmit={e => handleEditSubmit(e, msg.id)} className="edit-form">
                        <input
                          type="text"
                          value={editInput}
                          onChange={e => setEditInput(e.target.value)}
                          autoFocus
                          maxLength={2000}
                          className="edit-input"
                        />
                        <button type="submit" className="btn btn-primary btn-sm" disabled={!editInput.trim()}>Save</button>
                        <button type="button" className="btn btn-ghost btn-sm" onClick={cancelEdit}>Cancel</button>
                      </form>
                    ) : (
                      <div className="message-content">{msg.content}</div>
                    )}
                    <div className="message-reactions">
                      {(msg.reactions ?? []).map(r => {
                        const iReacted = currentUser ? r.userIds.includes(currentUser.id) : false;
                        const names = r.userIds.map(uid => getUserName(uid)).join(', ');
                        return (
                          <button
                            key={r.emoji}
                            className={`reaction-btn${iReacted ? ' reacted' : ''}`}
                            onClick={() => handleReaction(msg.id, r.emoji)}
                            title={names}
                          >
                            {r.emoji} {r.userIds.length}
                          </button>
                        );
                      })}
                      <div className="reaction-picker">
                        {['👍', '❤️', '😂', '😮', '😢'].map(emoji => (
                          <button
                            key={emoji}
                            className="reaction-add-btn"
                            onClick={() => handleReaction(msg.id, emoji)}
                            title={`React with ${emoji}`}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    </div>
                    {seenBy && <div className="message-reads">{seenBy}</div>}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            <div className="typing-indicator">
              {typingText}
            </div>

            {isMember ? (
              <div className="chat-input-area">
                {/* Pending scheduled messages for this room */}
                {scheduledMessages.filter(m => m.roomId === currentRoomId).length > 0 && (
                  <div className="scheduled-panel">
                    <div className="scheduled-panel-title">Scheduled</div>
                    {scheduledMessages.filter(m => m.roomId === currentRoomId).map(sm => (
                      <div key={sm.id} className="scheduled-item">
                        <span className="scheduled-content">{sm.content}</span>
                        <span className="scheduled-time">{new Date(sm.scheduledFor).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}</span>
                        <button
                          className="btn btn-ghost btn-sm btn-danger"
                          onClick={() => handleCancelScheduled(sm.id)}
                          title="Cancel scheduled message"
                        >Cancel</button>
                      </div>
                    ))}
                  </div>
                )}
                {scheduleMode ? (
                  <form className="chat-input-row schedule-row" onSubmit={handleSchedule}>
                    <input
                      type="text"
                      placeholder="Message to schedule..."
                      value={messageInput}
                      onChange={e => setMessageInput(e.target.value)}
                      maxLength={2000}
                      autoFocus
                    />
                    <input
                      type="datetime-local"
                      step="1"
                      min={getMinScheduleTime()}
                      value={scheduleTime}
                      onChange={e => setScheduleTime(e.target.value)}
                      title="Schedule time"
                    />
                    <button type="submit" disabled={!messageInput.trim() || !scheduleTime}>Schedule</button>
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => { setScheduleMode(false); setScheduleTime(''); }}>✕</button>
                  </form>
                ) : (
                  <form className="chat-input-row" onSubmit={handleSend}>
                    <input
                      type="text"
                      placeholder={expiresAfterSeconds !== null ? `Ephemeral message (disappears in ${expiresAfterSeconds >= 60 ? expiresAfterSeconds / 60 + 'm' : expiresAfterSeconds + 's'})...` : 'Type a message...'}
                      value={messageInput}
                      onChange={e => { setMessageInput(e.target.value); handleTyping(); }}
                      onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { handleSend(e); } }}
                      maxLength={2000}
                    />
                    <select
                      className="ephemeral-select"
                      value={expiresAfterSeconds ?? ''}
                      onChange={e => setExpiresAfterSeconds(e.target.value ? parseInt(e.target.value) : null)}
                      title="Send as ephemeral (disappearing) message"
                    >
                      <option value="">Normal</option>
                      <option value="60">⏱ 1 min</option>
                      <option value="300">⏱ 5 min</option>
                      <option value="3600">⏱ 1 hr</option>
                    </select>
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => setScheduleMode(true)} title="Schedule message">🕐</button>
                    <button type="submit" disabled={!messageInput.trim()}>Send</button>
                  </form>
                )}
              </div>
            ) : (
              <div className="not-member-notice">
                <span>You are not a member of this room.</span>
                <button className="btn btn-primary btn-sm" onClick={handleJoinRoom}>Join Room</button>
              </div>
            )}
          </>
        )}
      </div>
      {/* Edit history modal */}
      {historyMessageId !== null && (
        <div className="modal-overlay" onClick={closeHistory}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h3>Edit History</h3>
              <button className="btn btn-ghost btn-sm" onClick={closeHistory}>✕</button>
            </div>
            <div className="modal-body">
              {editHistory.length === 0 ? (
                <p style={{ color: 'var(--text-muted)' }}>No edit history available.</p>
              ) : (
                editHistory.map((edit, i) => (
                  <div key={edit.id} className="edit-history-item">
                    <div className="edit-history-meta">
                      <span>Version {i + 1}</span>
                      <span>{new Date(edit.editedAt).toLocaleString()}</span>
                    </div>
                    <div className="edit-history-content">{edit.previousContent}</div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
