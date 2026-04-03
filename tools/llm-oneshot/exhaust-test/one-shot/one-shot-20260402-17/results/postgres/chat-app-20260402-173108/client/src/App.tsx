import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ─── Types ─────────────────────────────────────────────────────────────────

type UserStatus = 'online' | 'away' | 'dnd' | 'invisible';

interface User {
  id: number;
  name: string;
  status: UserStatus;
  lastActive: string;
}

interface Room {
  id: number;
  name: string;
  creatorId: number;
}

interface RoomMember extends User {
  isAdmin: boolean;
  isBanned: boolean;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  userName: string;
  content: string;
  isEdited: boolean;
  isEphemeral: boolean;
  expiresAt: string | null;
  createdAt: string;
}

interface Reaction {
  id: number;
  messageId: number;
  userId: number;
  userName: string;
  emoji: string;
}

interface ReadReceipt {
  roomId: number;
  userId: number;
  userName: string;
  lastReadMessageId: number | null;
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  scheduledAt: string;
  sent: boolean;
}

// ─── Constants ─────────────────────────────────────────────────────────────

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

const STATUS_COLORS: Record<UserStatus, string> = {
  online: '#27ae60',
  away: '#f39c12',
  dnd: '#cc3b03',
  invisible: '#848484',
};

const STATUS_LABELS: Record<UserStatus, string> = {
  online: 'Online',
  away: 'Away',
  dnd: 'Do Not Disturb',
  invisible: 'Invisible',
};

// ─── Helpers ────────────────────────────────────────────────────────────────

function timeAgo(date: string) {
  const diff = Date.now() - new Date(date).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins} minute${mins !== 1 ? 's' : ''} ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hour${hrs !== 1 ? 's' : ''} ago`;
  const days = Math.floor(hrs / 24);
  return `${days} day${days !== 1 ? 's' : ''} ago`;
}

function formatTime(date: string) {
  return new Date(date).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ─── App ────────────────────────────────────────────────────────────────────

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [registerError, setRegisterError] = useState('');

  const [socket, setSocket] = useState<Socket | null>(null);
  const [connected, setConnected] = useState(false);

  const [rooms, setRooms] = useState<Room[]>([]);
  const [joinedRooms, setJoinedRooms] = useState<number[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<number | null>(null);
  const [messages, setMessages] = useState<Record<number, Message[]>>({});
  const [members, setMembers] = useState<RoomMember[]>([]);
  const [allUsers, setAllUsers] = useState<User[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [readReceipts, setReadReceipts] = useState<Record<number, ReadReceipt[]>>({});
  const [reactions, setReactions] = useState<Record<number, Reaction[]>>({});
  const [typingUsers, setTypingUsers] = useState<Record<number, { userId: number; userName: string }[]>>({});
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);

  const [newRoomName, setNewRoomName] = useState('');
  const [messageInput, setMessageInput] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [scheduleMode, setScheduleMode] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [showScheduled, setShowScheduled] = useState(false);

  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editContent, setEditContent] = useState('');
  const [editHistoryMsgId, setEditHistoryMsgId] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<{ id: number; content: string; editedAt: string }[]>([]);

  const [showMembers, setShowMembers] = useState(false);
  const [showReactPicker, setShowReactPicker] = useState<number | null>(null);

  const [kickedRoomIds, setKickedRoomIds] = useState<number[]>([]);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastReadRef = useRef<Record<number, number | null>>({});
  const inactivityTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Register / Login ────────────────────────────────────────────────────

  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!nameInput.trim()) return;
    setRegisterError('');
    try {
      const res = await fetch('/api/users/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: nameInput.trim() }),
      });
      if (!res.ok) {
        const err = await res.json();
        setRegisterError(err.error);
        return;
      }
      const user = await res.json();
      setCurrentUser(user);
    } catch {
      setRegisterError('Failed to connect to server');
    }
  };

  // ── Socket setup ─────────────────────────────────────────────────────────

  useEffect(() => {
    if (!currentUser) return;
    const s = io({ path: '/socket.io', transports: ['websocket', 'polling'] });
    setSocket(s);

    s.on('connect', () => {
      setConnected(true);
      s.emit('user:connect', currentUser.id);
    });
    s.on('disconnect', () => setConnected(false));

    return () => { s.disconnect(); };
  }, [currentUser]);

  // ── Initial data fetch ───────────────────────────────────────────────────

  useEffect(() => {
    if (!currentUser) return;
    fetch('/api/rooms').then(r => r.json()).then(setRooms);
    fetch('/api/users').then(r => r.json()).then(setAllUsers);
    fetch(`/api/users/${currentUser.id}/scheduled`).then(r => r.json()).then(setScheduledMessages);
  }, [currentUser]);

  // ── Socket events ────────────────────────────────────────────────────────

  useEffect(() => {
    if (!socket || !currentUser) return;

    socket.on('room:created', (room: Room) => {
      setRooms(prev => [...prev.filter(r => r.id !== room.id), room]);
    });

    socket.on('message:new', (msg: Message) => {
      setMessages(prev => {
        const roomMsgs = prev[msg.roomId] || [];
        if (roomMsgs.find(m => m.id === msg.id)) return prev;
        return { ...prev, [msg.roomId]: [...roomMsgs, msg] };
      });
      // Update unread if not in that room
      if (currentRoomIdRef.current !== msg.roomId) {
        setUnreadCounts(prev => ({ ...prev, [msg.roomId]: (prev[msg.roomId] || 0) + 1 }));
      }
    });

    socket.on('message:edited', (msg: Message) => {
      setMessages(prev => ({
        ...prev,
        [msg.roomId]: (prev[msg.roomId] || []).map(m => m.id === msg.id ? msg : m),
      }));
    });

    socket.on('message:deleted', ({ messageId, roomId }: { messageId: number; roomId: number }) => {
      setMessages(prev => ({
        ...prev,
        [roomId]: (prev[roomId] || []).filter(m => m.id !== messageId),
      }));
    });

    socket.on('reaction:updated', ({ messageId, reactions: rxns }: { messageId: number; reactions: Reaction[] }) => {
      setReactions(prev => ({ ...prev, [messageId]: rxns }));
    });

    socket.on('read:updated', (receipt: ReadReceipt) => {
      setReadReceipts(prev => {
        const roomReceipts = prev[receipt.roomId] || [];
        return {
          ...prev,
          [receipt.roomId]: [
            ...roomReceipts.filter(r => r.userId !== receipt.userId),
            receipt,
          ],
        };
      });
    });

    socket.on('user:status', (user: User) => {
      setAllUsers(prev => prev.map(u => u.id === user.id ? user : u));
      if (user.id === currentUser.id) setCurrentUser(user);
    });

    socket.on('typing:update', ({ roomId, userId, userName, isTyping }: { roomId: number; userId: number; userName: string; isTyping: boolean }) => {
      if (userId === currentUser.id) return;
      setTypingUsers(prev => {
        const room = prev[roomId] || [];
        if (isTyping) {
          if (room.find(u => u.userId === userId)) return prev;
          return { ...prev, [roomId]: [...room, { userId, userName }] };
        } else {
          return { ...prev, [roomId]: room.filter(u => u.userId !== userId) };
        }
      });
    });

    socket.on('room:kicked', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (userId === currentUser.id) {
        setKickedRoomIds(prev => [...prev, roomId]);
        setJoinedRooms(prev => prev.filter(id => id !== roomId));
        if (currentRoomIdRef.current === roomId) setCurrentRoomId(null);
      } else {
        setMembers(prev => prev.filter(m => m.id !== userId));
      }
    });

    socket.on('room:promoted', ({ roomId, userId }: { roomId: number; userId: number }) => {
      setMembers(prev => prev.map(m => m.id === userId ? { ...m, isAdmin: true } : m));
    });

    socket.on('room:member_joined', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (currentRoomIdRef.current === roomId) loadMembers(roomId);
    });

    socket.on('room:member_left', ({ roomId, userId }: { roomId: number; userId: number }) => {
      setMembers(prev => prev.filter(m => m.id !== userId));
    });

    socket.on('scheduled:sent', ({ id }: { id: number }) => {
      setScheduledMessages(prev => prev.filter(s => s.id !== id));
    });

    return () => {
      socket.off('room:created');
      socket.off('message:new');
      socket.off('message:edited');
      socket.off('message:deleted');
      socket.off('reaction:updated');
      socket.off('read:updated');
      socket.off('user:status');
      socket.off('typing:update');
      socket.off('room:kicked');
      socket.off('room:promoted');
      socket.off('room:member_joined');
      socket.off('room:member_left');
      socket.off('scheduled:sent');
    };
  }, [socket, currentUser]);

  // Track currentRoomId in a ref for use in callbacks
  const currentRoomIdRef = useRef<number | null>(null);
  useEffect(() => { currentRoomIdRef.current = currentRoomId; }, [currentRoomId]);

  // ── Room management ──────────────────────────────────────────────────────

  const loadMembers = useCallback(async (roomId: number) => {
    const res = await fetch(`/api/rooms/${roomId}/members`);
    const data = await res.json();
    setMembers(data);
  }, []);

  const loadMessages = useCallback(async (roomId: number) => {
    const res = await fetch(`/api/rooms/${roomId}/messages`);
    const msgs: Message[] = await res.json();
    setMessages(prev => ({ ...prev, [roomId]: msgs }));

    // Load reactions for each message
    for (const msg of msgs) {
      fetch(`/api/messages/${msg.id}/reactions`).then(r => r.json()).then(rxns => {
        setReactions(prev => ({ ...prev, [msg.id]: rxns }));
      });
    }

    // Load read receipts
    const rr = await fetch(`/api/rooms/${roomId}/receipts`);
    const receipts: ReadReceipt[] = await rr.json();
    setReadReceipts(prev => ({ ...prev, [roomId]: receipts }));

    return msgs;
  }, []);

  const joinRoom = useCallback(async (roomId: number) => {
    if (!currentUser) return;
    await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket?.emit('room:join', roomId);
    setJoinedRooms(prev => [...new Set([...prev, roomId])]);
  }, [currentUser, socket]);

  const enterRoom = useCallback(async (roomId: number) => {
    if (!currentUser) return;
    if (!joinedRooms.includes(roomId)) {
      await joinRoom(roomId);
    }
    setCurrentRoomId(roomId);
    setShowMembers(false);
    setUnreadCounts(prev => ({ ...prev, [roomId]: 0 }));

    const msgs = await loadMessages(roomId);
    await loadMembers(roomId);

    // Mark as read
    const lastMsg = msgs[msgs.length - 1];
    const lastMsgId = lastMsg?.id || null;
    lastReadRef.current[roomId] = lastMsgId;
    await fetch(`/api/rooms/${roomId}/read`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, lastMessageId: lastMsgId }),
    });
  }, [currentUser, joinedRooms, joinRoom, loadMessages, loadMembers]);

  const createRoom = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentUser || !newRoomName.trim()) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    if (res.ok) {
      const room = await res.json();
      setNewRoomName('');
      setJoinedRooms(prev => [...new Set([...prev, room.id])]);
      socket?.emit('room:join', room.id);
      await enterRoom(room.id);
    }
  };

  const leaveRoom = async (roomId: number) => {
    if (!currentUser) return;
    await fetch(`/api/rooms/${roomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socket?.emit('room:leave', roomId);
    setJoinedRooms(prev => prev.filter(id => id !== roomId));
    if (currentRoomId === roomId) setCurrentRoomId(null);
  };

  // ── Message actions ──────────────────────────────────────────────────────

  const sendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!currentUser || !currentRoomId || !messageInput.trim()) return;

    if (scheduleMode && scheduleTime) {
      const res = await fetch(`/api/rooms/${currentRoomId}/schedule`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content: messageInput.trim(), scheduledAt: scheduleTime }),
      });
      if (res.ok) {
        const scheduled = await res.json();
        setScheduledMessages(prev => [...prev, scheduled]);
        setMessageInput('');
        setScheduleMode(false);
        setScheduleTime('');
      }
      return;
    }

    const res = await fetch(`/api/rooms/${currentRoomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        userId: currentUser.id,
        content: messageInput.trim(),
        isEphemeral,
        ephemeralDuration: isEphemeral ? ephemeralDuration : undefined,
      }),
    });
    if (res.ok) {
      setMessageInput('');
      socket?.emit('typing:stop', { roomId: currentRoomId, userId: currentUser.id });
      if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    }

    // Reset activity
    resetInactivityTimer();
  };

  const handleTyping = (e: React.ChangeEvent<HTMLInputElement>) => {
    setMessageInput(e.target.value);
    if (!currentUser || !currentRoomId) return;
    socket?.emit('typing:start', { roomId: currentRoomId, userId: currentUser.id });
    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socket?.emit('typing:stop', { roomId: currentRoomId, userId: currentUser.id });
    }, 3000);
    resetInactivityTimer();
  };

  const startEdit = (msg: Message) => {
    setEditingMessageId(msg.id);
    setEditContent(msg.content);
  };

  const saveEdit = async (messageId: number) => {
    if (!currentUser) return;
    await fetch(`/api/messages/${messageId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: editContent }),
    });
    setEditingMessageId(null);
  };

  const viewHistory = async (messageId: number) => {
    const res = await fetch(`/api/messages/${messageId}/history`);
    const data = await res.json();
    setEditHistory(data);
    setEditHistoryMsgId(messageId);
  };

  const toggleReaction = async (messageId: number, emoji: string) => {
    if (!currentUser) return;
    await fetch(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
    setShowReactPicker(null);
  };

  // ── Members / permissions ────────────────────────────────────────────────

  const kickUser = async (targetUserId: number) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    await loadMembers(currentRoomId);
  };

  const promoteUser = async (targetUserId: number) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    await loadMembers(currentRoomId);
  };

  // ── Status ────────────────────────────────────────────────────────────────

  const updateStatus = async (status: UserStatus) => {
    if (!currentUser) return;
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
  };

  // ── Inactivity auto-away ──────────────────────────────────────────────────

  const resetInactivityTimer = useCallback(() => {
    if (!currentUser) return;
    if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
    // If user was away due to inactivity, restore to online
    if (currentUser.status === 'away') {
      updateStatus('online');
    }
    inactivityTimerRef.current = setTimeout(() => {
      updateStatus('away');
    }, 5 * 60 * 1000);
  }, [currentUser]);

  useEffect(() => {
    if (!currentUser) return;
    const handler = () => resetInactivityTimer();
    document.addEventListener('mousemove', handler);
    document.addEventListener('keydown', handler);
    resetInactivityTimer();
    return () => {
      document.removeEventListener('mousemove', handler);
      document.removeEventListener('keydown', handler);
    };
  }, [currentUser, resetInactivityTimer]);

  // ── Read receipt on room view ─────────────────────────────────────────────

  useEffect(() => {
    if (!currentUser || !currentRoomId) return;
    const msgs = messages[currentRoomId] || [];
    const lastMsg = msgs[msgs.length - 1];
    if (!lastMsg) return;
    if (lastReadRef.current[currentRoomId] === lastMsg.id) return;
    lastReadRef.current[currentRoomId] = lastMsg.id;
    fetch(`/api/rooms/${currentRoomId}/read`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, lastMessageId: lastMsg.id }),
    });
  }, [messages, currentRoomId, currentUser]);

  // ── Auto-scroll ───────────────────────────────────────────────────────────

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, currentRoomId]);

  // ── Cancel scheduled ──────────────────────────────────────────────────────

  const cancelScheduled = async (id: number) => {
    if (!currentUser) return;
    await fetch(`/api/scheduled/${id}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setScheduledMessages(prev => prev.filter(s => s.id !== id));
  };

  // ── Derived state ─────────────────────────────────────────────────────────

  const currentRoom = rooms.find(r => r.id === currentRoomId);
  const currentMessages = currentRoomId ? (messages[currentRoomId] || []) : [];
  const currentTyping = currentRoomId ? (typingUsers[currentRoomId] || []) : [];
  const currentReceipts = currentRoomId ? (readReceipts[currentRoomId] || []) : [];
  const isCurrentRoomAdmin = members.find(m => m.id === currentUser?.id)?.isAdmin || false;

  // Group messages by sender for display
  const groupedMessages = currentMessages.reduce<Array<Message & { showHeader: boolean }>>((acc, msg, i) => {
    const prev = currentMessages[i - 1];
    const showHeader = !prev || prev.userId !== msg.userId ||
      (new Date(msg.createdAt).getTime() - new Date(prev.createdAt).getTime()) > 5 * 60 * 1000;
    acc.push({ ...msg, showHeader });
    return acc;
  }, []);

  // ── Countdown for ephemeral messages ──────────────────────────────────────

  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  // ── Render ────────────────────────────────────────────────────────────────

  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>PostgreSQL Chat</h1>
          <p className="login-subtitle">Real-time chat powered by PostgreSQL</p>
          <form onSubmit={handleRegister}>
            <input
              type="text"
              placeholder="Enter your name"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              autoFocus
            />
            {registerError && <div className="error-msg">{registerError}</div>}
            <button type="submit">Join</button>
          </form>
        </div>
      </div>
    );
  }

  if (!connected) {
    return (
      <div className="connecting-screen">
        <div className="spinner" />
        <p>Connecting to server...</p>
      </div>
    );
  }

  return (
    <div className="app">
      {/* ── Sidebar ──────────────────────────────────────────────────── */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1 className="app-title">PostgreSQL Chat</h1>
          <div className="user-info">
            <span className="status-dot" style={{ backgroundColor: STATUS_COLORS[currentUser.status] }} />
            <span className="user-name">{currentUser.name}</span>
          </div>
          <select
            className="status-selector"
            value={currentUser.status}
            onChange={e => updateStatus(e.target.value as UserStatus)}
            title="Set status"
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        {/* Room creation */}
        <form className="create-room-form" onSubmit={createRoom}>
          <input
            type="text"
            placeholder="New room name"
            value={newRoomName}
            onChange={e => setNewRoomName(e.target.value)}
          />
          <button type="submit" title="Create room">+</button>
        </form>

        {/* Room list */}
        <div className="section-label">Rooms</div>
        <ul className="room-list">
          {rooms.map(room => (
            <li
              key={room.id}
              className={`room-item ${currentRoomId === room.id ? 'active' : ''} ${kickedRoomIds.includes(room.id) ? 'kicked' : ''}`}
              onClick={() => !kickedRoomIds.includes(room.id) && enterRoom(room.id)}
            >
              <span className="room-name"># {room.name}</span>
              {(unreadCounts[room.id] || 0) > 0 && (
                <span className="unread-badge">{unreadCounts[room.id]}</span>
              )}
            </li>
          ))}
          {rooms.length === 0 && (
            <li className="empty-state">Create a room to get started</li>
          )}
        </ul>

        {/* Online users */}
        <div className="section-label">Users</div>
        <ul className="user-list">
          {allUsers.map(user => (
            <li key={user.id} className="user-item">
              <span className="status-dot" style={{ backgroundColor: STATUS_COLORS[user.status] }} />
              <span className="user-item-name">{user.name}</span>
              {(user.status === 'away' || user.status === 'dnd' || user.status === 'invisible') && (
                <span className="last-active">Last active {timeAgo(user.lastActive)}</span>
              )}
            </li>
          ))}
        </ul>
      </aside>

      {/* ── Main area ────────────────────────────────────────────────── */}
      <main className="main">
        {!currentRoom ? (
          <div className="empty-main">
            <p>Select a room to start chatting</p>
            <p className="empty-hint">Or create a new room using the + button</p>
          </div>
        ) : (
          <>
            {/* Room header */}
            <div className="room-header">
              <div className="room-header-left">
                <h2># {currentRoom.name}</h2>
              </div>
              <div className="room-header-right">
                <button onClick={() => setShowScheduled(s => !s)} className="header-btn">
                  Scheduled {scheduledMessages.filter(s => s.roomId === currentRoomId).length > 0
                    ? `(${scheduledMessages.filter(s => s.roomId === currentRoomId).length})`
                    : ''}
                </button>
                <button onClick={() => setShowMembers(s => !s)} className="header-btn">Members</button>
                <button onClick={() => leaveRoom(currentRoomId!)} className="header-btn leave-btn">Leave</button>
              </div>
            </div>

            {/* Kicked notification */}
            {kickedRoomIds.includes(currentRoomId!) && (
              <div className="kicked-banner">You have been kicked from this room.</div>
            )}

            {/* Scheduled messages panel */}
            {showScheduled && (
              <div className="scheduled-panel">
                <h3>Scheduled Messages</h3>
                {scheduledMessages.filter(s => s.roomId === currentRoomId).length === 0 ? (
                  <p className="empty-state">No pending scheduled messages</p>
                ) : (
                  scheduledMessages.filter(s => s.roomId === currentRoomId).map(s => (
                    <div key={s.id} className="scheduled-item">
                      <div className="scheduled-text">
                        <span className="scheduled-badge">Scheduled</span>
                        {s.content}
                      </div>
                      <div className="scheduled-meta">
                        Sends at {new Date(s.scheduledAt).toLocaleString()}
                      </div>
                      <div className="scheduled-actions">
                        <span className="pending-label">Pending</span>
                        <button onClick={() => cancelScheduled(s.id)} className="cancel-btn">Cancel</button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            )}

            {/* Members panel */}
            {showMembers && (
              <div className="members-panel">
                <h3>Members</h3>
                {members.map(member => (
                  <div key={member.id} className="member-item">
                    <span className="status-dot" style={{ backgroundColor: STATUS_COLORS[member.status] }} />
                    <span className="member-name">{member.name}</span>
                    {member.isAdmin && <span className="admin-badge">Admin</span>}
                    {isCurrentRoomAdmin && member.id !== currentUser.id && (
                      <div className="member-actions">
                        <button onClick={() => kickUser(member.id)} className="kick-btn">Kick</button>
                        {!member.isAdmin && (
                          <button onClick={() => promoteUser(member.id)} className="promote-btn">Promote</button>
                        )}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}

            {/* Messages */}
            <div className="messages-container" onClick={() => setShowReactPicker(null)}>
              {currentMessages.length === 0 && (
                <div className="empty-messages">No messages yet. Say hello!</div>
              )}
              {groupedMessages.map(msg => {
                const msgReactions = reactions[msg.id] || [];
                const reactionGroups = EMOJIS.reduce<Record<string, { count: number; users: string[]; reacted: boolean }>>((acc, emoji) => {
                  const r = msgReactions.filter(r => r.emoji === emoji);
                  if (r.length > 0) {
                    acc[emoji] = {
                      count: r.length,
                      users: r.map(rx => rx.userName),
                      reacted: r.some(rx => rx.userId === currentUser.id),
                    };
                  }
                  return acc;
                }, {});

                const seenBy = currentReceipts
                  .filter(r => r.userId !== currentUser.id && r.lastReadMessageId && r.lastReadMessageId >= msg.id)
                  .map(r => r.userName);

                const isLastMessage = msg.id === currentMessages[currentMessages.length - 1]?.id;

                let countdown: string | null = null;
                if (msg.expiresAt) {
                  const remaining = new Date(msg.expiresAt).getTime() - now;
                  if (remaining > 0) {
                    const secs = Math.ceil(remaining / 1000);
                    countdown = secs >= 60 ? `${Math.floor(secs / 60)}m ${secs % 60}s` : `${secs}s`;
                  }
                }

                return (
                  <div key={msg.id} className={`message-group ${msg.isEphemeral ? 'ephemeral' : ''}`}>
                    {msg.showHeader && (
                      <div className="message-header">
                        <span className="message-sender" style={{ color: getSenderColor(msg.userId) }}>
                          {msg.userName}
                        </span>
                        <span className="message-time">{formatTime(msg.createdAt)}</span>
                      </div>
                    )}
                    <div className="message-body">
                      {editingMessageId === msg.id ? (
                        <div className="edit-form">
                          <input
                            type="text"
                            value={editContent}
                            onChange={e => setEditContent(e.target.value)}
                            onKeyDown={e => {
                              if (e.key === 'Enter') saveEdit(msg.id);
                              if (e.key === 'Escape') setEditingMessageId(null);
                            }}
                            autoFocus
                          />
                          <button onClick={() => saveEdit(msg.id)} className="save-btn">Save</button>
                          <button onClick={() => setEditingMessageId(null)} className="cancel-btn">Cancel</button>
                        </div>
                      ) : (
                        <span className="message-content">
                          {msg.content}
                          {msg.isEdited && (
                            <button className="edited-indicator" onClick={() => viewHistory(msg.id)}>(edited)</button>
                          )}
                          {countdown && (
                            <span className="ephemeral-indicator"> · expires in {countdown}</span>
                          )}
                          {msg.isEphemeral && !countdown && (
                            <span className="ephemeral-indicator"> · disappearing</span>
                          )}
                        </span>
                      )}
                      <div className="message-actions">
                        {EMOJIS.map(emoji => (
                          <button
                            key={emoji}
                            className="react-btn"
                            onClick={e => { e.stopPropagation(); toggleReaction(msg.id, emoji); }}
                            title={`React with ${emoji}`}
                          >
                            {emoji}
                          </button>
                        ))}
                        {msg.userId === currentUser.id && !editingMessageId && (
                          <button className="edit-btn" onClick={() => startEdit(msg)}>Edit</button>
                        )}
                      </div>
                    </div>

                    {/* Reactions display */}
                    {Object.entries(reactionGroups).length > 0 && (
                      <div className="reactions">
                        {Object.entries(reactionGroups).map(([emoji, data]) => (
                          <button
                            key={emoji}
                            className={`reaction-pill ${data.reacted ? 'reacted' : ''}`}
                            onClick={() => toggleReaction(msg.id, emoji)}
                            title={data.users.join(', ')}
                          >
                            {emoji} {data.count}
                          </button>
                        ))}
                      </div>
                    )}

                    {/* Read receipts under last message */}
                    {isLastMessage && seenBy.length > 0 && (
                      <div className="read-receipts">
                        Seen by {seenBy.join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            <div className="typing-indicator">
              {currentTyping.length === 1 && (
                <span>{currentTyping[0].userName} is typing...</span>
              )}
              {currentTyping.length === 2 && (
                <span>{currentTyping[0].userName} and {currentTyping[1].userName} are typing...</span>
              )}
              {currentTyping.length > 2 && (
                <span>Multiple users are typing...</span>
              )}
            </div>

            {/* Message input */}
            <form className="message-form" onSubmit={sendMessage}>
              <div className="input-controls">
                <label className="ephemeral-label" title="Ephemeral message">
                  <input
                    type="checkbox"
                    checked={isEphemeral}
                    onChange={e => setIsEphemeral(e.target.checked)}
                  />
                  Ephemeral
                </label>
                {isEphemeral && (
                  <select
                    value={ephemeralDuration}
                    onChange={e => setEphemeralDuration(parseInt(e.target.value))}
                    className="duration-select"
                    title="Expire duration"
                  >
                    <option value={30}>30s</option>
                    <option value={60}>1m</option>
                    <option value={300}>5m</option>
                  </select>
                )}
                <button
                  type="button"
                  className={`schedule-btn ${scheduleMode ? 'active' : ''}`}
                  onClick={() => setScheduleMode(s => !s)}
                  title="Schedule message"
                  aria-label="schedule"
                >
                  Schedule
                </button>
                {scheduleMode && (
                  <input
                    type="datetime-local"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                    className="schedule-time-input"
                  />
                )}
              </div>
              <div className="input-row">
                <input
                  ref={inputRef}
                  type="text"
                  placeholder="Type a message..."
                  value={messageInput}
                  onChange={handleTyping}
                  onKeyDown={e => {
                    if (e.key === 'Escape') {
                      setEditingMessageId(null);
                      setScheduleMode(false);
                    }
                  }}
                />
                <button type="submit">Send</button>
              </div>
            </form>
          </>
        )}
      </main>

      {/* Edit history modal */}
      {editHistoryMsgId !== null && (
        <div className="modal-backdrop" onClick={() => setEditHistoryMsgId(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            {editHistory.length === 0 ? (
              <p>No previous versions</p>
            ) : (
              editHistory.map(h => (
                <div key={h.id} className="history-item">
                  <div className="history-content">{h.content}</div>
                  <div className="history-time">{new Date(h.editedAt).toLocaleString()}</div>
                </div>
              ))
            )}
            <button onClick={() => setEditHistoryMsgId(null)} className="close-btn">Close</button>
          </div>
        </div>
      )}
    </div>
  );
}

// Deterministic color from user ID
function getSenderColor(userId: number): string {
  const colors = ['#7289da', '#43b581', '#faa61a', '#f47fff', '#00b0f4', '#99aab5'];
  return colors[userId % colors.length];
}
