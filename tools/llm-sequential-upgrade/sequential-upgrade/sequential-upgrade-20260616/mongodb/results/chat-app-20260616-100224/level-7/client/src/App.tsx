import React, { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

interface Room {
  _id: string;
  name: string;
  createdBy: string;
  members: string[];
  admins: string[];
  banned: string[];
}

interface Reaction {
  emoji: string;
  users: string[];
}

interface EditEntry {
  text: string;
  editedAt: string;
}

interface Message {
  _id: string;
  roomId: string;
  sender: string;
  text: string;
  createdAt: string;
  readBy: string[];
  expiresAt?: string;
  reactions: Reaction[];
  editHistory: EditEntry[];
  isEdited: boolean;
}

interface ScheduledMessage {
  _id: string;
  roomId: string;
  sender: string;
  text: string;
  scheduledAt: string;
  sent: boolean;
}

type UserStatus = 'online' | 'away' | 'dnd' | 'invisible';

interface UserInfo {
  name: string;
  status: UserStatus;
  lastSeen: string;
}

const TYPING_STOP_DELAY = 2000;
const AUTO_AWAY_MS = 5 * 60 * 1000;

function lastActiveLabel(lastSeen: string): string {
  const ms = Date.now() - new Date(lastSeen).getTime();
  const minutes = Math.floor(ms / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} minute${minutes === 1 ? '' : 's'} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hour${hours === 1 ? '' : 's'} ago`;
  const days = Math.floor(hours / 24);
  return `${days} day${days === 1 ? '' : 's'} ago`;
}

function statusDotClass(status: UserStatus): string {
  return `status-dot ${status}`;
}

export default function App() {
  const [userName, setUserName] = useState<string>(() => localStorage.getItem('chat-username') || '');
  const [nameInput, setNameInput] = useState('');
  const [nameError, setNameError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [onlineUsersData, setOnlineUsersData] = useState<UserInfo[]>([]);
  const [typingUsers, setTypingUsers] = useState<string[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<string, number>>({});
  const [newRoomName, setNewRoomName] = useState('');
  const [messageText, setMessageText] = useState('');
  const [isConnected, setIsConnected] = useState(false);
  const [createRoomError, setCreateRoomError] = useState('');
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [showScheduler, setShowScheduler] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [ephemeralDuration, setEphemeralDuration] = useState(0);
  const [showEphemeral, setShowEphemeral] = useState(false);
  const [, setTick] = useState(0);
  const [hoveredMsgId, setHoveredMsgId] = useState<string | null>(null);
  const [editingMsgId, setEditingMsgId] = useState<string | null>(null);
  const [editText, setEditText] = useState('');
  const [historyMsgId, setHistoryMsgId] = useState<string | null>(null);
  const [showMembersPanel, setShowMembersPanel] = useState(false);
  const [kickedMessage, setKickedMessage] = useState<string | null>(null);
  const [userStatus, setUserStatus] = useState<UserStatus>('online');

  const socketRef = useRef<Socket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<number | null>(null);
  const isTypingRef = useRef(false);
  const currentRoomIdRef = useRef<string | null>(null);
  const userNameRef = useRef<string>(userName);
  const userStatusRef = useRef<UserStatus>('online');
  const isAutoAwayRef = useRef(false);
  const autoAwayTimerRef = useRef<number | null>(null);

  useEffect(() => { currentRoomIdRef.current = currentRoomId; }, [currentRoomId]);
  useEffect(() => { userNameRef.current = userName; }, [userName]);
  useEffect(() => { userStatusRef.current = userStatus; }, [userStatus]);

  const hasEphemeral = messages.some((m) => m.expiresAt);
  useEffect(() => {
    if (!hasEphemeral) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [hasEphemeral]);

  const currentRoom = rooms.find((r) => r._id === currentRoomId) ?? null;

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const markRead = useCallback((roomId: string) => {
    const uname = userNameRef.current;
    if (!uname) return;
    fetch(`/api/rooms/${roomId}/read`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userName: uname }),
    }).catch(() => undefined);
  }, []);

  const patchStatus = useCallback(async (status: UserStatus) => {
    const uname = userNameRef.current;
    if (!uname) return;
    setUserStatus(status);
    userStatusRef.current = status;
    await fetch(`/api/users/${encodeURIComponent(uname)}/status`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    }).catch(() => undefined);
  }, []);

  // Auto-away: detect inactivity and set away after AUTO_AWAY_MS
  useEffect(() => {
    if (!userName) return;

    const scheduleAutoAway = () => {
      if (autoAwayTimerRef.current !== null) clearTimeout(autoAwayTimerRef.current);
      autoAwayTimerRef.current = window.setTimeout(() => {
        if (userStatusRef.current === 'online') {
          isAutoAwayRef.current = true;
          patchStatus('away');
        }
      }, AUTO_AWAY_MS);
    };

    const onActivity = () => {
      if (isAutoAwayRef.current) {
        isAutoAwayRef.current = false;
        patchStatus('online');
      }
      scheduleAutoAway();
    };

    document.addEventListener('mousemove', onActivity);
    document.addEventListener('keydown', onActivity);
    document.addEventListener('click', onActivity);
    scheduleAutoAway();

    return () => {
      document.removeEventListener('mousemove', onActivity);
      document.removeEventListener('keydown', onActivity);
      document.removeEventListener('click', onActivity);
      if (autoAwayTimerRef.current !== null) clearTimeout(autoAwayTimerRef.current);
    };
  }, [userName, patchStatus]);

  const handleReact = useCallback(async (messageId: string, emoji: string) => {
    await fetch(`/api/messages/${messageId}/react`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userName: userNameRef.current, emoji }),
    });
  }, []);

  const startEdit = useCallback((msg: Message) => {
    setEditingMsgId(msg._id);
    setEditText(msg.text);
  }, []);

  const cancelEdit = useCallback(() => {
    setEditingMsgId(null);
    setEditText('');
  }, []);

  const submitEdit = useCallback(async (messageId: string) => {
    const text = editText.trim();
    if (!text) return;
    await fetch(`/api/messages/${messageId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userName: userNameRef.current, text }),
    });
    setEditingMsgId(null);
    setEditText('');
  }, [editText]);

  const handleKick = useCallback(async (targetUser: string) => {
    if (!currentRoomIdRef.current) return;
    await fetch(`/api/rooms/${currentRoomIdRef.current}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminUser: userNameRef.current, targetUser }),
    });
  }, []);

  const handlePromote = useCallback(async (targetUser: string) => {
    if (!currentRoomIdRef.current) return;
    await fetch(`/api/rooms/${currentRoomIdRef.current}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminUser: userNameRef.current, targetUser }),
    });
  }, []);

  useEffect(() => {
    if (!userName) return;

    const socket = io({ path: '/socket.io' });
    socketRef.current = socket;

    socket.on('connect', () => {
      setIsConnected(true);
      socket.emit('authenticate', { userName });
    });

    socket.on('disconnect', () => setIsConnected(false));

    socket.on('online-users', ({ users }: { users: UserInfo[] }) => {
      setOnlineUsersData(users);
      const self = users.find((u) => u.name === userNameRef.current);
      if (self && !isAutoAwayRef.current) {
        setUserStatus(self.status);
        userStatusRef.current = self.status;
      }
    });

    socket.on('message', ({ message }: { message: Message }) => {
      if (message.roomId === currentRoomIdRef.current) {
        setMessages((prev) => {
          if (prev.find((m) => m._id === message._id)) return prev;
          return [...prev, message];
        });
        markRead(message.roomId);
      } else {
        setUnreadCounts((prev) => ({
          ...prev,
          [message.roomId]: (prev[message.roomId] ?? 0) + 1,
        }));
      }
    });

    socket.on('typing-update', ({ roomId, typingUsers: users }: { roomId: string; typingUsers: string[] }) => {
      if (roomId === currentRoomIdRef.current) {
        setTypingUsers(users.filter((u) => u !== userNameRef.current));
      }
    });

    socket.on('read-receipts-updated', ({ roomId, messages: updated }: { roomId: string; messages: Message[] }) => {
      if (roomId === currentRoomIdRef.current) {
        setMessages(updated);
      }
    });

    socket.on('room-created', ({ room }: { room: Room }) => {
      setRooms((prev) => (prev.find((r) => r._id === room._id) ? prev : [...prev, room]));
    });

    socket.on('room-updated', ({ room }: { room: Room }) => {
      setRooms((prev) => prev.map((r) => (r._id === room._id ? room : r)));
    });

    socket.on('scheduled-message-sent', ({ scheduledId }: { scheduledId: string }) => {
      setScheduledMessages((prev) => prev.filter((m) => m._id !== scheduledId));
    });

    socket.on('message-deleted', ({ messageId }: { messageId: string }) => {
      setMessages((prev) => prev.filter((m) => m._id !== messageId));
    });

    socket.on('reaction-updated', ({ message }: { message: Message }) => {
      if (message.roomId === currentRoomIdRef.current) {
        setMessages((prev) => prev.map((m) => m._id === message._id ? { ...message } : m));
      }
    });

    socket.on('message-updated', ({ message }: { message: Message }) => {
      if (message.roomId === currentRoomIdRef.current) {
        setMessages((prev) => prev.map((m) => m._id === message._id ? { ...message } : m));
      }
    });

    socket.on('kicked-from-room', ({ roomId, roomName }: { roomId: string; roomName: string }) => {
      setShowMembersPanel(false);
      if (currentRoomIdRef.current === roomId) {
        setCurrentRoomId(null);
        setMessages([]);
        setTypingUsers([]);
        setScheduledMessages([]);
        setShowScheduler(false);
        setScheduleTime('');
        setShowEphemeral(false);
        setEphemeralDuration(0);
        setKickedMessage(`You have been kicked from #${roomName}`);
      }
      setRooms((prev) => prev.map((r) =>
        r._id === roomId ? { ...r, members: r.members.filter((m) => m !== userNameRef.current) } : r
      ));
    });

    Promise.all([
      fetch('/api/rooms').then((r) => r.json()),
      fetch('/api/users').then((r) => r.json()),
    ]).then(([roomsData, usersData]) => {
      const loadedRooms: Room[] = roomsData.rooms ?? [];
      setRooms(loadedRooms);
      const usersArr: UserInfo[] = usersData.users ?? [];
      setOnlineUsersData(usersArr);
      const self = usersArr.find((u) => u.name === userName);
      if (self) { setUserStatus(self.status); userStatusRef.current = self.status; }

      const memberRooms = loadedRooms.filter((r) => r.members.includes(userName));
      memberRooms.forEach((room) => socket.emit('join-room', room._id));

      Promise.all(
        memberRooms.map((room) =>
          fetch(`/api/rooms/${room._id}/unread?userName=${encodeURIComponent(userName)}`)
            .then((r) => r.json())
            .then((d: { count: number }) => ({ roomId: room._id, count: d.count ?? 0 }))
        )
      ).then((counts) => {
        const map: Record<string, number> = {};
        counts.forEach(({ roomId, count }) => { map[roomId] = count; });
        setUnreadCounts(map);
      });
    });

    return () => { socket.disconnect(); };
  }, [userName, markRead]);

  const stopTyping = useCallback(() => {
    const roomId = currentRoomIdRef.current;
    if (!roomId) return;
    if (typingTimerRef.current !== null) {
      clearTimeout(typingTimerRef.current);
      typingTimerRef.current = null;
    }
    if (isTypingRef.current) {
      isTypingRef.current = false;
      socketRef.current?.emit('typing-stop', { roomId });
    }
  }, []);

  const selectRoom = useCallback(async (roomId: string) => {
    if (currentRoomIdRef.current === roomId) return;
    stopTyping();
    setCurrentRoomId(roomId);
    setMessages([]);
    setTypingUsers([]);
    setScheduledMessages([]);
    setShowScheduler(false);
    setScheduleTime('');
    setShowEphemeral(false);
    setEphemeralDuration(0);
    setShowMembersPanel(false);
    setKickedMessage(null);
    setUnreadCounts((prev) => ({ ...prev, [roomId]: 0 }));
    socketRef.current?.emit('join-room', roomId);
    const uname = userNameRef.current;
    const [msgData, schedData] = await Promise.all([
      fetch(`/api/rooms/${roomId}/messages`).then((r) => r.json()),
      fetch(`/api/rooms/${roomId}/scheduled?userName=${encodeURIComponent(uname)}`).then((r) => r.json()),
    ]);
    setMessages(msgData.messages ?? []);
    setScheduledMessages(schedData.scheduled ?? []);
    markRead(roomId);
  }, [stopTyping, markRead]);

  const handleSetName = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = nameInput.trim();
    if (!name) { setNameError('Please enter a name'); return; }
    if (name.length > 32) { setNameError('Name must be 32 characters or less'); return; }
    const res = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name }),
    });
    if (!res.ok) {
      const err = await res.json();
      setNameError(err.error ?? 'Failed to set name');
      return;
    }
    localStorage.setItem('chat-username', name);
    setUserName(name);
  };

  const handleCreateRoom = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = newRoomName.trim();
    if (!name) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, createdBy: userName }),
    });
    if (!res.ok) {
      const err = await res.json();
      setCreateRoomError(err.error ?? 'Failed to create room');
      return;
    }
    const { room } = await res.json();
    setNewRoomName('');
    setCreateRoomError('');
    socketRef.current?.emit('join-room', room._id);
    await selectRoom(room._id);
  };

  const handleJoinRoom = async (roomId: string) => {
    const res = await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userName }),
    });
    if (res.ok) {
      const { room } = await res.json();
      setRooms((prev) => prev.map((r) => (r._id === roomId ? room : r)));
      socketRef.current?.emit('join-room', roomId);
      await selectRoom(roomId);
    }
  };

  const handleLeaveRoom = async (roomId: string) => {
    stopTyping();
    await fetch(`/api/rooms/${roomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userName }),
    });
    socketRef.current?.emit('leave-room', roomId);
    if (currentRoomId === roomId) {
      setCurrentRoomId(null);
      setMessages([]);
      setTypingUsers([]);
      setScheduledMessages([]);
      setShowScheduler(false);
      setScheduleTime('');
      setShowEphemeral(false);
      setEphemeralDuration(0);
      setShowMembersPanel(false);
    }
  };

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!messageText.trim() || !currentRoomId) return;
    const text = messageText.trim();
    setMessageText('');
    stopTyping();
    if (scheduleTime) {
      const res = await fetch(`/api/rooms/${currentRoomId}/scheduled`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sender: userName, text, scheduledAt: new Date(scheduleTime).toISOString() }),
      });
      if (res.ok) {
        const { scheduled } = await res.json();
        setScheduledMessages((prev) => [...prev, scheduled]);
        setScheduleTime('');
        setShowScheduler(false);
      }
    } else {
      const body: { sender: string; text: string; ttlSeconds?: number } = { sender: userName, text };
      if (ephemeralDuration > 0) body.ttlSeconds = ephemeralDuration;
      await fetch(`/api/rooms/${currentRoomId}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
    }
  };

  const handleCancelScheduled = async (id: string) => {
    await fetch(`/api/scheduled/${id}`, { method: 'DELETE' });
    setScheduledMessages((prev) => prev.filter((m) => m._id !== id));
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setMessageText(e.target.value);
    const roomId = currentRoomIdRef.current;
    if (!roomId) return;
    if (!isTypingRef.current) {
      isTypingRef.current = true;
      socketRef.current?.emit('typing-start', { roomId });
    }
    if (typingTimerRef.current !== null) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = window.setTimeout(() => {
      isTypingRef.current = false;
      typingTimerRef.current = null;
      socketRef.current?.emit('typing-stop', { roomId });
    }, TYPING_STOP_DELAY);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      setMessageText('');
      stopTyping();
    }
  };

  const handleStatusChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const status = e.target.value as UserStatus;
    isAutoAwayRef.current = false;
    await patchStatus(status);
  };

  const visibleOnlineUsers = onlineUsersData.filter((u) =>
    u.name !== userName || userStatus !== 'invisible'
  );

  if (!userName) {
    return (
      <div className="name-entry-screen">
        <div className="name-entry-card">
          <h1>MongoDB Chat</h1>
          <p>Enter a display name to get started</p>
          <form onSubmit={handleSetName}>
            <input
              type="text"
              value={nameInput}
              onChange={(e) => { setNameInput(e.target.value); setNameError(''); }}
              placeholder="Your display name"
              maxLength={32}
              autoFocus
            />
            {nameError && <div className="error">{nameError}</div>}
            <button type="submit">Join Chat</button>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="sidebar">
        <div className="sidebar-header">
          <h1>MongoDB Chat</h1>
          <span className={`status-dot ${isConnected ? 'online' : 'offline'}`} title={isConnected ? 'Connected' : 'Connecting...'} />
        </div>

        <div className="user-info">
          <span className={statusDotClass(userStatus)} title={userStatus} />
          <span className="user-name">{userName}</span>
          <select
            className="status-select"
            value={userStatus}
            onChange={handleStatusChange}
            title="Set your status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        <div className="section">
          <div className="section-title">Rooms</div>
          <form onSubmit={handleCreateRoom} className="create-room-form">
            <input
              type="text"
              value={newRoomName}
              onChange={(e) => { setNewRoomName(e.target.value); setCreateRoomError(''); }}
              placeholder="New room name"
              maxLength={64}
            />
            <button type="submit">+</button>
          </form>
          {createRoomError && <div className="error" style={{ padding: '0 12px 4px' }}>{createRoomError}</div>}

          <div className="room-list">
            {rooms.length === 0 && (
              <div className="empty-state">Create a room to get started</div>
            )}
            {rooms.map((room) => {
              const isMember = room.members.includes(userName);
              const isActive = room._id === currentRoomId;
              const unread = unreadCounts[room._id] ?? 0;
              return (
                <div
                  key={room._id}
                  className={`room-item ${isActive ? 'active' : ''}`}
                  onClick={() => (isMember ? selectRoom(room._id) : handleJoinRoom(room._id))}
                >
                  <span className="room-name"># {room.name}</span>
                  <div className="room-meta">
                    {!isMember && <span className="join-hint">Join</span>}
                    {unread > 0 && <span className="unread-badge">{unread > 99 ? '99+' : unread}</span>}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <div className="section">
          <div className="section-title">Online — {visibleOnlineUsers.filter((u) => u.status !== 'invisible').length}</div>
          <div className="online-users">
            {visibleOnlineUsers.map((u) => {
              const showLastActive = u.status === 'away' || u.status === 'invisible';
              return (
                <div key={u.name} className="online-user">
                  <span className={statusDotClass(u.status)} title={u.status} />
                  <div className="online-user-info">
                    <span>{u.name}</span>
                    {showLastActive && (
                      <span className="last-active">Last active {lastActiveLabel(u.lastSeen)}</span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {historyMsgId && (() => {
        const hMsg = messages.find((m) => m._id === historyMsgId);
        if (!hMsg) return null;
        return (
          <div className="modal-backdrop" onClick={() => setHistoryMsgId(null)}>
            <div className="modal-panel" onClick={(e) => e.stopPropagation()}>
              <div className="modal-header">
                <span>Edit History</span>
                <button type="button" className="modal-close-btn" onClick={() => setHistoryMsgId(null)}>✕</button>
              </div>
              <div className="modal-body">
                {hMsg.editHistory.length === 0 ? (
                  <div className="empty-state">No edit history</div>
                ) : (
                  hMsg.editHistory.map((entry, idx) => (
                    <div key={idx} className="history-entry">
                      <span className="history-time">
                        {new Date(entry.editedAt).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}
                      </span>
                      <span className="history-text">{entry.text}</span>
                    </div>
                  ))
                )}
                <div className="history-entry current">
                  <span className="history-time">Current</span>
                  <span className="history-text">{hMsg.text}</span>
                </div>
              </div>
            </div>
          </div>
        );
      })()}

      {showMembersPanel && currentRoom && (
        <div className="modal-backdrop" onClick={() => setShowMembersPanel(false)}>
          <div className="modal-panel" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <span>Members — #{currentRoom.name}</span>
              <button type="button" className="modal-close-btn" onClick={() => setShowMembersPanel(false)}>✕</button>
            </div>
            <div className="modal-body">
              {currentRoom.members.length === 0 ? (
                <div className="empty-state">No members</div>
              ) : (
                currentRoom.members.map((member) => {
                  const isAdmin = (currentRoom.admins ?? []).includes(member);
                  const isCurrentUserAdmin = (currentRoom.admins ?? []).includes(userName);
                  const isSelf = member === userName;
                  const memberInfo = onlineUsersData.find((u) => u.name === member);
                  const mStatus = memberInfo?.status ?? 'offline';
                  return (
                    <div key={member} className="member-item">
                      <span className={`status-dot ${mStatus}`} title={mStatus} />
                      <span className="member-name">{member}</span>
                      {isAdmin && <span className="admin-badge">Admin</span>}
                      {isCurrentUserAdmin && !isSelf && !isAdmin && (
                        <>
                          <button
                            type="button"
                            className="promote-btn"
                            onClick={() => handlePromote(member)}
                          >
                            Promote
                          </button>
                          <button
                            type="button"
                            className="kick-btn"
                            onClick={() => handleKick(member)}
                          >
                            Kick
                          </button>
                        </>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </div>
      )}

      <div className="main">
        {!currentRoom ? (
          <div className="no-room">
            {kickedMessage ? (
              <div className="kicked-notice">
                <span>⚠️ {kickedMessage}</span>
                <button type="button" className="modal-close-btn" onClick={() => setKickedMessage(null)}>✕</button>
              </div>
            ) : (
              <div className="empty-state">
                Select a room to start chatting, or create a new one
              </div>
            )}
          </div>
        ) : (
          <>
            <div className="room-header">
              <span className="room-header-name"># {currentRoom.name}</span>
              <span className="room-member-count">
                {currentRoom.members.length} member{currentRoom.members.length !== 1 ? 's' : ''}
              </span>
              <button
                type="button"
                className="members-btn"
                onClick={() => setShowMembersPanel((v) => !v)}
              >
                Members
              </button>
              <button className="leave-btn" onClick={() => handleLeaveRoom(currentRoom._id)}>
                Leave
              </button>
            </div>

            <div className="messages-container">
              {messages.length === 0 ? (
                <div className="empty-state">No messages yet. Say something!</div>
              ) : (
                messages.map((msg, i) => {
                  const prev = messages[i - 1];
                  const isGrouped =
                    prev &&
                    prev.sender === msg.sender &&
                    new Date(msg.createdAt).getTime() - new Date(prev.createdAt).getTime() < 60000;
                  const seenBy = msg.readBy.filter((u) => u !== msg.sender);
                  const isOwn = msg.sender === userName;

                  const remainingSec = msg.expiresAt
                    ? Math.max(0, Math.floor((new Date(msg.expiresAt).getTime() - Date.now()) / 1000))
                    : null;
                  const expiryLabel =
                    remainingSec !== null
                      ? remainingSec >= 60
                        ? `${Math.floor(remainingSec / 60)}m ${remainingSec % 60}s`
                        : `${remainingSec}s`
                      : null;

                  const reactions = msg.reactions ?? [];
                  const isHovered = hoveredMsgId === msg._id;
                  const isEditing = editingMsgId === msg._id;
                  const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

                  return (
                    <div
                      key={msg._id}
                      className={`message${isGrouped ? ' grouped' : ''}${msg.expiresAt ? ' ephemeral' : ''}`}
                      onMouseEnter={() => setHoveredMsgId(msg._id)}
                      onMouseLeave={() => setHoveredMsgId(null)}
                    >
                      {!isGrouped && (
                        <div className="message-header">
                          <span className="sender-name">{msg.sender}</span>
                          <span className="timestamp">
                            {new Date(msg.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                          </span>
                          {expiryLabel !== null && (
                            <span className={`ephemeral-badge${remainingSec !== null && remainingSec < 30 ? ' urgent' : ''}`}>
                              ⏱ {expiryLabel}
                            </span>
                          )}
                          {msg.isEdited && (
                            <button
                              type="button"
                              className="edited-indicator"
                              onClick={() => setHistoryMsgId(msg._id)}
                              title="View edit history"
                            >(edited)</button>
                          )}
                          {isHovered && (
                            <div className="msg-actions">
                              {isOwn && (
                                <button
                                  type="button"
                                  className="edit-msg-btn"
                                  onClick={() => startEdit(msg)}
                                  title="Edit message"
                                >✏️</button>
                              )}
                              <div className="emoji-picker">
                                {EMOJIS.map((e) => (
                                  <button
                                    key={e}
                                    type="button"
                                    className="emoji-pick-btn"
                                    onClick={() => handleReact(msg._id, e)}
                                    title={`React with ${e}`}
                                  >{e}</button>
                                ))}
                              </div>
                            </div>
                          )}
                        </div>
                      )}
                      {isGrouped && (
                        <div className="message-header grouped-hover">
                          {expiryLabel !== null && (
                            <span className={`ephemeral-badge inline${remainingSec !== null && remainingSec < 30 ? ' urgent' : ''}`}>
                              ⏱ {expiryLabel}
                            </span>
                          )}
                          {msg.isEdited && (
                            <button
                              type="button"
                              className="edited-indicator"
                              onClick={() => setHistoryMsgId(msg._id)}
                              title="View edit history"
                            >(edited)</button>
                          )}
                          {isHovered && (
                            <div className="msg-actions">
                              {isOwn && (
                                <button
                                  type="button"
                                  className="edit-msg-btn"
                                  onClick={() => startEdit(msg)}
                                  title="Edit message"
                                >✏️</button>
                              )}
                              <div className="emoji-picker">
                                {EMOJIS.map((e) => (
                                  <button
                                    key={e}
                                    type="button"
                                    className="emoji-pick-btn"
                                    onClick={() => handleReact(msg._id, e)}
                                    title={`React with ${e}`}
                                  >{e}</button>
                                ))}
                              </div>
                            </div>
                          )}
                        </div>
                      )}
                      {isEditing ? (
                        <div className="edit-input-row">
                          <input
                            type="text"
                            className="edit-input"
                            value={editText}
                            onChange={(e) => setEditText(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') { e.preventDefault(); submitEdit(msg._id); }
                              if (e.key === 'Escape') cancelEdit();
                            }}
                            autoFocus
                            maxLength={2000}
                          />
                          <button type="button" className="edit-save-btn" onClick={() => submitEdit(msg._id)}>Save</button>
                          <button type="button" className="edit-cancel-btn" onClick={cancelEdit}>Cancel</button>
                        </div>
                      ) : (
                        <div className="message-text">{msg.text}</div>
                      )}
                      {isOwn && seenBy.length > 0 && (
                        <div className="read-receipt">Seen by {seenBy.join(', ')}</div>
                      )}
                      {reactions.length > 0 && (
                        <div className="reactions-row">
                          {reactions.map((r) => {
                            const isMine = r.users.includes(userName);
                            return (
                              <button
                                key={r.emoji}
                                type="button"
                                className={`reaction-btn${isMine ? ' mine' : ''}`}
                                onClick={() => handleReact(msg._id, r.emoji)}
                                title={r.users.join(', ')}
                              >
                                {r.emoji} {r.users.length}
                              </button>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })
              )}

              {typingUsers.length > 0 && (
                <div className="typing-indicator">
                  {typingUsers.length === 1
                    ? `${typingUsers[0]} is typing...`
                    : `${typingUsers.slice(0, -1).join(', ')} and ${typingUsers[typingUsers.length - 1]} are typing...`}
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>

            {scheduledMessages.length > 0 && (
              <div className="scheduled-panel">
                <div className="scheduled-panel-header">Scheduled ({scheduledMessages.length})</div>
                {scheduledMessages.map((sm) => (
                  <div key={sm._id} className="scheduled-item">
                    <span className="scheduled-time">
                      {new Date(sm.scheduledAt).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}
                    </span>
                    <span className="scheduled-text">{sm.text}</span>
                    <button
                      className="cancel-scheduled-btn"
                      onClick={() => handleCancelScheduled(sm._id)}
                      title="Cancel scheduled message"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            )}

            {showScheduler && (
              <div className="scheduler-row">
                <span className="scheduler-label">Send at:</span>
                <input
                  type="datetime-local"
                  className="schedule-time-input"
                  value={scheduleTime}
                  min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}
                  onChange={(e) => setScheduleTime(e.target.value)}
                />
                <button
                  type="button"
                  className="cancel-scheduler-btn"
                  onClick={() => { setShowScheduler(false); setScheduleTime(''); }}
                >
                  Cancel
                </button>
              </div>
            )}

            {showEphemeral && (
              <div className="ephemeral-row">
                <span className="ephemeral-row-label">Disappears after:</span>
                {([60, 300, 600] as const).map((secs) => {
                  const label = secs === 60 ? '1 min' : secs === 300 ? '5 min' : '10 min';
                  return (
                    <button
                      key={secs}
                      type="button"
                      className={`ephemeral-option-btn${ephemeralDuration === secs ? ' active' : ''}`}
                      onClick={() => setEphemeralDuration(ephemeralDuration === secs ? 0 : secs)}
                    >
                      {label}
                    </button>
                  );
                })}
                <button
                  type="button"
                  className="cancel-scheduler-btn"
                  onClick={() => { setShowEphemeral(false); setEphemeralDuration(0); }}
                >
                  Cancel
                </button>
              </div>
            )}

            <form className="input-area" onSubmit={handleSendMessage}>
              <input
                type="text"
                value={messageText}
                onChange={handleInputChange}
                onKeyDown={handleKeyDown}
                placeholder={`Message #${currentRoom.name}`}
                maxLength={2000}
                autoFocus
              />
              <button
                type="button"
                className={`schedule-toggle-btn${showScheduler ? ' active' : ''}`}
                onClick={() => setShowScheduler((v) => !v)}
                title="Schedule message"
              >
                🕐
              </button>
              <button
                type="button"
                className={`ephemeral-toggle-btn${showEphemeral || ephemeralDuration > 0 ? ' active' : ''}`}
                onClick={() => setShowEphemeral((v) => !v)}
                title="Send ephemeral message"
              >
                🔥
              </button>
              <button type="submit" disabled={!messageText.trim() || (showScheduler && !scheduleTime)}>
                {showScheduler && scheduleTime ? 'Schedule' : 'Send'}
              </button>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
