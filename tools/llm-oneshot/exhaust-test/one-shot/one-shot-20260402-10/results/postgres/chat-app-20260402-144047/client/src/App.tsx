import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ─── Types ───────────────────────────────────────────────────────────────────

type UserStatus = 'online' | 'away' | 'dnd' | 'invisible';

interface User {
  id: string;
  name: string;
  status: UserStatus;
  lastActive: string;
}

interface Room {
  id: string;
  name: string;
  creatorId: string;
}

interface RoomMember {
  user: User;
  member: { roomId: string; userId: string; isAdmin: boolean; isBanned: boolean };
}

interface Reaction {
  id: string;
  messageId: string;
  userId: string;
  emoji: string;
}

interface Message {
  id: string;
  roomId: string;
  userId: string;
  content: string;
  isEdited: boolean;
  expiresAt: string | null;
  createdAt: string;
  reactions: Reaction[];
  readBy: { userId: string; name: string }[];
  userName?: string;
}

interface ScheduledMessage {
  id: string;
  roomId: string;
  userId: string;
  content: string;
  scheduledAt: string;
  isCancelled: boolean;
}

// ─── Socket ──────────────────────────────────────────────────────────────────

const socket: Socket = io({ path: '/socket.io' });

// ─── App ─────────────────────────────────────────────────────────────────────

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [registerError, setRegisterError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<string | null>(null);
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);

  const [messages, setMessages] = useState<Message[]>([]);
  const [messageInput, setMessageInput] = useState('');
  const [ephemeralSeconds, setEphemeralSeconds] = useState<number>(0);

  const [members, setMembers] = useState<RoomMember[]>([]);
  const [allUsers, setAllUsers] = useState<User[]>([]);
  const [unread, setUnread] = useState<Record<string, number>>({});

  const [typingUsers, setTypingUsers] = useState<Record<string, Set<string>>>({}); // roomId -> Set<userId>
  const [editingMsgId, setEditingMsgId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');
  const [historyMsgId, setHistoryMsgId] = useState<string | null>(null);
  const [editHistory, setEditHistory] = useState<{ id: string; content: string; editedAt: string }[]>([]);

  const [scheduledList, setScheduledList] = useState<ScheduledMessage[]>([]);
  const [showScheduled, setShowScheduled] = useState(false);
  const [scheduleInput, setScheduleInput] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');

  const [showMembers, setShowMembers] = useState(false);

  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // ─── Init ─────────────────────────────────────────────────────────────────

  useEffect(() => {
    fetchRooms();
  }, []);

  useEffect(() => {
    if (!currentUser) return;
    socket.emit('identify', currentUser.id);
    fetchAllUsers();
    fetchUnread();

    socket.on('userStatusChanged', (u: User) => {
      setAllUsers(prev => prev.map(x => x.id === u.id ? u : x));
    });
    socket.on('roomCreated', (room: Room) => {
      setRooms(prev => [...prev.filter(r => r.id !== room.id), room]);
    });
    socket.on('newMessage', (msg: Message) => {
      setMessages(prev => {
        if (prev.find(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      setUnread(prev => {
        if (msg.roomId === currentRoomId) return prev;
        if (msg.userId === currentUser?.id) return prev;
        return { ...prev, [msg.roomId]: (prev[msg.roomId] ?? 0) + 1 };
      });
    });
    socket.on('messageEdited', (updated: Message) => {
      setMessages(prev => prev.map(m => m.id === updated.id ? { ...m, content: updated.content, isEdited: true } : m));
    });
    socket.on('messageDeleted', ({ messageId }: { messageId: string }) => {
      setMessages(prev => prev.filter(m => m.id !== messageId));
    });
    socket.on('reactionsUpdated', ({ messageId, reactions }: { messageId: string; reactions: { reaction: Reaction; user: User }[] }) => {
      setMessages(prev => prev.map(m => m.id === messageId ? { ...m, reactions: reactions.map(r => r.reaction) } : m));
    });
    socket.on('userTyping', ({ roomId, userId }: { roomId: string; userId: string }) => {
      setTypingUsers(prev => {
        const set = new Set(prev[roomId] ?? []);
        set.add(userId);
        return { ...prev, [roomId]: set };
      });
    });
    socket.on('userStoppedTyping', ({ roomId, userId }: { roomId: string; userId: string }) => {
      setTypingUsers(prev => {
        const set = new Set(prev[roomId] ?? []);
        set.delete(userId);
        return { ...prev, [roomId]: set };
      });
    });
    socket.on('messagesRead', ({ roomId, userId: readerId }: { roomId: string; userId: string }) => {
      if (readerId === currentUser?.id) return;
      // Update readBy for messages in that room
      setMessages(prev => prev.map(m => {
        if (m.roomId !== roomId) return m;
        if (m.readBy.find(r => r.userId === readerId)) return m;
        const readerUser = allUsers.find(u => u.id === readerId);
        return { ...m, readBy: [...m.readBy, { userId: readerId, name: readerUser?.name ?? readerId }] };
      }));
    });
    socket.on('userKicked', ({ roomId, userId }: { roomId: string; userId: string }) => {
      if (userId === currentUser?.id) {
        if (currentRoomId === roomId) setCurrentRoomId(null);
        alert('You have been kicked from the room.');
        setRooms(prev => prev); // trigger re-render
      }
      setMembers(prev => prev.filter(m => m.user.id !== userId));
    });
    socket.on('memberJoined', () => {
      if (currentRoomId) fetchMembers(currentRoomId);
    });
    socket.on('memberLeft', () => {
      if (currentRoomId) fetchMembers(currentRoomId);
    });
    socket.on('memberPromoted', () => {
      if (currentRoomId) fetchMembers(currentRoomId);
    });

    return () => {
      socket.off('userStatusChanged');
      socket.off('roomCreated');
      socket.off('newMessage');
      socket.off('messageEdited');
      socket.off('messageDeleted');
      socket.off('reactionsUpdated');
      socket.off('userTyping');
      socket.off('userStoppedTyping');
      socket.off('messagesRead');
      socket.off('userKicked');
      socket.off('memberJoined');
      socket.off('memberLeft');
      socket.off('memberPromoted');
    };
  }, [currentUser, currentRoomId, allUsers]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // ─── Fetch helpers ────────────────────────────────────────────────────────

  const fetchRooms = async () => {
    const res = await fetch('/api/rooms');
    setRooms(await res.json());
  };

  const fetchAllUsers = async () => {
    const res = await fetch('/api/users');
    setAllUsers(await res.json());
  };

  const fetchUnread = async () => {
    if (!currentUser) return;
    const res = await fetch(`/api/users/${currentUser.id}/unread`);
    setUnread(await res.json());
  };

  const fetchMembers = useCallback(async (roomId: string) => {
    const res = await fetch(`/api/rooms/${roomId}/members`);
    setMembers(await res.json());
  }, []);

  const fetchMessages = useCallback(async (roomId: string) => {
    if (!currentUser) return;
    const res = await fetch(`/api/rooms/${roomId}/messages?userId=${currentUser.id}`);
    const msgs = await res.json();
    setMessages(msgs);
    setUnread(prev => ({ ...prev, [roomId]: 0 }));
  }, [currentUser]);

  const fetchScheduled = useCallback(async () => {
    if (!currentUser) return;
    const res = await fetch(`/api/users/${currentUser.id}/scheduled`);
    setScheduledList(await res.json());
  }, [currentUser]);

  // ─── Handlers ─────────────────────────────────────────────────────────────

  const handleRegister = async () => {
    setRegisterError('');
    const res = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: nameInput }),
    });
    if (!res.ok) {
      const err = await res.json();
      setRegisterError(err.error);
      return;
    }
    const user = await res.json();
    setCurrentUser(user);
  };

  const handleCreateRoom = async () => {
    if (!currentUser || !newRoomName.trim()) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    if (res.ok) {
      const room = await res.json();
      setNewRoomName('');
      setShowCreateRoom(false);
      enterRoom(room.id);
    }
  };

  const enterRoom = async (roomId: string) => {
    if (!currentUser) return;
    // Join via API
    await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket.emit('leaveRoom', currentRoomId);
    socket.emit('joinRoom', roomId);
    setCurrentRoomId(roomId);
    await fetchMessages(roomId);
    await fetchMembers(roomId);
    socket.emit('markRead', { roomId, userId: currentUser.id });
    setUnread(prev => ({ ...prev, [roomId]: 0 }));
    socket.emit('activity');
  };

  const handleLeaveRoom = async () => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket.emit('leaveRoom', currentRoomId);
    setCurrentRoomId(null);
    setMessages([]);
    setMembers([]);
  };

  const handleSendMessage = async () => {
    if (!currentUser || !currentRoomId || !messageInput.trim()) return;
    await fetch(`/api/rooms/${currentRoomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: messageInput, expiresInSeconds: ephemeralSeconds || undefined }),
    });
    setMessageInput('');
    setEphemeralSeconds(0);
    socket.emit('stopTyping', { roomId: currentRoomId, userId: currentUser.id });
    socket.emit('activity');
  };

  const handleTyping = () => {
    if (!currentUser || !currentRoomId) return;
    socket.emit('typing', { roomId: currentRoomId, userId: currentUser.id });
    socket.emit('activity');
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socket.emit('stopTyping', { roomId: currentRoomId, userId: currentUser.id });
    }, 3000);
  };

  const handleEditSave = async () => {
    if (!currentUser || !editingMsgId || !editContent.trim()) return;
    await fetch(`/api/messages/${editingMsgId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: editContent }),
    });
    setEditingMsgId(null);
    setEditContent('');
  };

  const handleReact = async (messageId: string, emoji: string) => {
    if (!currentUser) return;
    await fetch(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
  };

  const handleShowHistory = async (messageId: string) => {
    const res = await fetch(`/api/messages/${messageId}/history`);
    setEditHistory(await res.json());
    setHistoryMsgId(messageId);
  };

  const handleSchedule = async () => {
    if (!currentUser || !currentRoomId || !scheduleInput.trim() || !scheduleTime) return;
    await fetch(`/api/rooms/${currentRoomId}/scheduled`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: scheduleInput, scheduledAt: scheduleTime }),
    });
    setScheduleInput('');
    setScheduleTime('');
    fetchScheduled();
  };

  const handleCancelScheduled = async (id: string) => {
    await fetch(`/api/scheduled/${id}`, { method: 'DELETE' });
    fetchScheduled();
  };

  const handleStatusChange = async (status: UserStatus) => {
    if (!currentUser) return;
    const res = await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
    const updated = await res.json();
    setCurrentUser(updated);
  };

  const handleKick = async (targetUserId: string) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    fetchMembers(currentRoomId);
  };

  const handlePromote = async (targetUserId: string) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    fetchMembers(currentRoomId);
  };

  // ─── Helpers ──────────────────────────────────────────────────────────────

  const getStatusColor = (status: UserStatus) => {
    switch (status) {
      case 'online': return '#27ae60';
      case 'away': return '#f26522';
      case 'dnd': return '#cc3b03';
      case 'invisible': return '#848484';
    }
  };

  const getStatusDot = (status: UserStatus) => (
    <span style={{ display: 'inline-block', width: 10, height: 10, borderRadius: '50%', background: getStatusColor(status), marginRight: 6 }} />
  );

  const formatLastActive = (lastActive: string) => {
    const diff = Date.now() - new Date(lastActive).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins} minute${mins === 1 ? '' : 's'} ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs} hour${hrs === 1 ? '' : 's'} ago`;
    return `${Math.floor(hrs / 24)} day(s) ago`;
  };

  const groupReactions = (reactionList: Reaction[]) => {
    const map = new Map<string, string[]>();
    for (const r of reactionList) {
      const existing = map.get(r.emoji) ?? [];
      map.set(r.emoji, [...existing, r.userId]);
    }
    return map;
  };

  const currentRoom = rooms.find(r => r.id === currentRoomId);
  const myMembership = members.find(m => m.user.id === currentUser?.id);
  const isAdmin = myMembership?.member?.isAdmin ?? false;
  const typingInRoom = currentRoomId ? [...(typingUsers[currentRoomId] ?? [])] : [];
  const typingNames = typingInRoom
    .filter(uid => uid !== currentUser?.id)
    .map(uid => allUsers.find(u => u.id === uid)?.name ?? uid);

  // ─── Register screen ──────────────────────────────────────────────────────

  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-box">
          <h1>PostgreSQL Chat</h1>
          <p>Enter your display name to get started</p>
          <input
            className="input"
            placeholder="Your name"
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            maxLength={30}
          />
          {registerError && <p className="error">{registerError}</p>}
          <button className="btn btn-primary" onClick={handleRegister} type="submit">Join</button>
        </div>
      </div>
    );
  }

  // ─── Main Layout ──────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h2>PostgreSQL Chat</h2>
          <div className="user-info">
            {getStatusDot(currentUser.status)}
            <span>{currentUser.name}</span>
          </div>
          <select
            className="status-select"
            value={currentUser.status}
            onChange={e => handleStatusChange(e.target.value as UserStatus)}
            aria-label="Set status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="btn btn-small" onClick={() => setShowCreateRoom(v => !v)}>+ New</button>
          </div>
          {showCreateRoom && (
            <div className="create-room">
              <input
                className="input input-small"
                placeholder="Room name"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                maxLength={50}
              />
              <button className="btn btn-primary btn-small" onClick={handleCreateRoom}>Create</button>
            </div>
          )}
          <ul className="room-list">
            {rooms.map(room => (
              <li
                key={room.id}
                className={`room-item ${room.id === currentRoomId ? 'active' : ''}`}
                onClick={() => enterRoom(room.id)}
              >
                <span className="room-name">{room.name}</span>
                {unread[room.id] > 0 && (
                  <span className="badge">{unread[room.id]}</span>
                )}
              </li>
            ))}
          </ul>
        </div>

        <div className="sidebar-section online-users">
          <div className="section-header"><span>Users</span></div>
          <ul className="user-list">
            {allUsers.map(u => (
              <li key={u.id} className="user-item">
                {getStatusDot(u.status)}
                <span>{u.name}</span>
                {u.status !== 'online' && u.status !== 'invisible' && (
                  <span className="last-active">Last active {formatLastActive(u.lastActive)}</span>
                )}
              </li>
            ))}
          </ul>
        </div>
      </aside>

      {/* Main chat area */}
      <main className="chat-area">
        {!currentRoom ? (
          <div className="no-room">
            <p>Select or create a room to start chatting</p>
          </div>
        ) : (
          <>
            <div className="chat-header">
              <h3>#{currentRoom.name}</h3>
              <div className="chat-header-actions">
                <button className="btn btn-small" onClick={() => { setShowMembers(v => !v); }}>Members</button>
                <button className="btn btn-small" onClick={() => { fetchScheduled(); setShowScheduled(v => !v); }}>Scheduled</button>
                <button className="btn btn-danger btn-small" onClick={handleLeaveRoom}>Leave</button>
              </div>
            </div>

            {showMembers && (
              <div className="members-panel">
                <h4>Members</h4>
                {members.map(({ user: u, member: m }) => (
                  <div key={u.id} className="member-row">
                    {getStatusDot(u.status)}
                    <span>{u.name}</span>
                    {m.isAdmin && <span className="admin-badge">Admin</span>}
                    {isAdmin && u.id !== currentUser.id && (
                      <>
                        <button className="btn btn-danger btn-tiny" onClick={() => handleKick(u.id)}>Kick</button>
                        {!m.isAdmin && (
                          <button className="btn btn-small btn-tiny" onClick={() => handlePromote(u.id)}>Promote</button>
                        )}
                      </>
                    )}
                  </div>
                ))}
              </div>
            )}

            {showScheduled && (
              <div className="scheduled-panel">
                <h4>Scheduled</h4>
                <div className="schedule-form">
                  <input
                    className="input input-small"
                    placeholder="Message content"
                    value={scheduleInput}
                    onChange={e => setScheduleInput(e.target.value)}
                  />
                  <input
                    type="datetime-local"
                    className="input input-small"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                  />
                  <button className="btn btn-primary btn-small" onClick={handleSchedule}>Schedule</button>
                </div>
                <ul>
                  {scheduledList.filter(s => s.roomId === currentRoomId && !s.isCancelled).map(s => (
                    <li key={s.id} className="scheduled-item">
                      <span>{s.content}</span>
                      <span className="muted"> @ {new Date(s.scheduledAt).toLocaleString()}</span>
                      <span className="pending-label"> Pending</span>
                      <button className="btn btn-danger btn-tiny" onClick={() => handleCancelScheduled(s.id)}>Cancel</button>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <div className="messages">
              {messages.map(msg => {
                const isOwn = msg.userId === currentUser.id;
                const sender = allUsers.find(u => u.id === msg.userId);
                const grouped = groupReactions(msg.reactions);
                const timeLeft = msg.expiresAt ? Math.max(0, Math.ceil((new Date(msg.expiresAt).getTime() - Date.now()) / 1000)) : null;

                return (
                  <div key={msg.id} className={`message ${isOwn ? 'own' : ''}`}>
                    <div className="message-header">
                      <span className="message-author">{sender?.name ?? msg.userName ?? msg.userId}</span>
                      <span className="message-time">{new Date(msg.createdAt).toLocaleTimeString()}</span>
                      {timeLeft !== null && (
                        <span className="ephemeral-indicator">expires in {timeLeft}s</span>
                      )}
                    </div>

                    {editingMsgId === msg.id ? (
                      <div className="edit-form">
                        <input
                          className="input"
                          value={editContent}
                          onChange={e => setEditContent(e.target.value)}
                          onKeyDown={e => e.key === 'Enter' && handleEditSave()}
                        />
                        <button className="btn btn-primary btn-small" onClick={handleEditSave}>Save</button>
                        <button className="btn btn-small" onClick={() => setEditingMsgId(null)}>Cancel</button>
                      </div>
                    ) : (
                      <div className="message-body">
                        <span className="message-content">{msg.content}</span>
                        {msg.isEdited && (
                          <span
                            className="edited-indicator"
                            onClick={() => handleShowHistory(msg.id)}
                            style={{ cursor: 'pointer' }}
                          >(edited)</span>
                        )}
                        {isOwn && (
                          <button
                            className="btn btn-tiny edit-btn"
                            onClick={() => { setEditingMsgId(msg.id); setEditContent(msg.content); }}
                          >Edit</button>
                        )}
                      </div>
                    )}

                    {/* Reactions */}
                    <div className="reactions">
                      {['👍', '❤️', '😂', '😮', '😢'].map(emoji => {
                        const voters = grouped.get(emoji) ?? [];
                        const voterNames = voters.map(uid => allUsers.find(u => u.id === uid)?.name ?? uid).join(', ');
                        return (
                          <button
                            key={emoji}
                            className={`reaction-btn ${voters.includes(currentUser.id) ? 'reacted' : ''}`}
                            onClick={() => handleReact(msg.id, emoji)}
                            title={voterNames || emoji}
                          >
                            {emoji}{voters.length > 0 && ` ${voters.length}`}
                          </button>
                        );
                      })}
                    </div>

                    {/* Read receipts */}
                    {msg.readBy.length > 0 && (
                      <div className="read-receipts">
                        Seen by {msg.readBy.map(r => r.name).join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Edit history modal */}
            {historyMsgId && (
              <div className="modal-overlay" onClick={() => setHistoryMsgId(null)}>
                <div className="modal" onClick={e => e.stopPropagation()}>
                  <h4>Edit History</h4>
                  {editHistory.length === 0 ? <p>No history</p> : editHistory.map(h => (
                    <div key={h.id} className="history-item">
                      <span className="muted">{new Date(h.editedAt).toLocaleString()}</span>
                      <p>{h.content}</p>
                    </div>
                  ))}
                  <button className="btn btn-small" onClick={() => setHistoryMsgId(null)}>Close</button>
                </div>
              </div>
            )}

            {/* Typing indicator */}
            {typingNames.length > 0 && (
              <div className="typing-indicator">
                {typingNames.length === 1
                  ? `${typingNames[0]} is typing...`
                  : `Multiple users are typing...`}
              </div>
            )}

            {/* Message input */}
            <div className="message-input-area">
              <div className="ephemeral-row">
                <label htmlFor="ephemeral-select">Ephemeral:</label>
                <select
                  id="ephemeral-select"
                  className="input input-small"
                  value={ephemeralSeconds}
                  onChange={e => setEphemeralSeconds(Number(e.target.value))}
                >
                  <option value={0}>Off</option>
                  <option value={30}>30s</option>
                  <option value={60}>1m</option>
                  <option value={300}>5m</option>
                </select>
              </div>
              <div className="input-row">
                <input
                  className="input message-input"
                  placeholder="Type a message..."
                  value={messageInput}
                  onChange={e => { setMessageInput(e.target.value); handleTyping(); }}
                  onKeyDown={e => e.key === 'Enter' && handleSendMessage()}
                  maxLength={2000}
                />
                <button className="btn btn-primary" onClick={handleSendMessage}>Send</button>
              </div>
            </div>
          </>
        )}
      </main>
    </div>
  );
}
