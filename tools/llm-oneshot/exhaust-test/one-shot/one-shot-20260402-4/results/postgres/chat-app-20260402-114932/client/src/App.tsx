import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

interface User {
  id: number;
  username: string;
  status: string;
  lastActive: string;
  isOnline?: boolean;
}

interface Room {
  id: number;
  name: string;
  createdBy: number;
  createdAt: string;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  isEdited: boolean;
  isEphemeral: boolean;
  ephemeralExpiresAt: string | null;
  createdAt: string;
  updatedAt: string;
}

interface Reaction {
  id: number;
  messageId: number;
  userId: number;
  emoji: string;
}

interface ReadReceipt {
  messageId: number;
  userId: number;
}

interface RoomMember {
  id: number;
  roomId: number;
  userId: number;
  isAdmin: boolean;
  isBanned: boolean;
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  scheduledFor: string;
  isSent: boolean;
  isCancelled: boolean;
}

interface EditHistory {
  id: number;
  messageId: number;
  oldContent: string;
  newContent: string;
  editedAt: string;
}

type TypingMap = Record<number, { username: string; timeout: ReturnType<typeof setTimeout> }>;

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function formatTime(dateStr: string): string {
  return new Date(dateStr).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [usernameInput, setUsernameInput] = useState('');
  const [rooms, setRooms] = useState<Room[]>([]);
  const [users, setUsers] = useState<User[]>([]);
  const [currentRoom, setCurrentRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [messageInput, setMessageInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [typing, setTyping] = useState<TypingMap>({});
  const [readReceipts, setReadReceipts] = useState<Record<number, number[]>>({});
  const [reactions, setReactions] = useState<Record<number, Reaction[]>>({});
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [scheduledMsgs, setScheduledMsgs] = useState<ScheduledMessage[]>([]);
  const [scheduleContent, setScheduleContent] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralMinutes, setEphemeralMinutes] = useState(1);
  const [editingMsgId, setEditingMsgId] = useState<number | null>(null);
  const [editContent, setEditContent] = useState('');
  const [editHistory, setEditHistory] = useState<EditHistory[] | null>(null);
  const [showScheduled, setShowScheduled] = useState(false);
  const [showMembers, setShowMembers] = useState(false);
  const [reactionHover, setReactionHover] = useState<number | null>(null);
  const [ephemeralCountdowns, setEphemeralCountdowns] = useState<Record<number, number>>({});
  const [joinedRooms, setJoinedRooms] = useState<Set<number>>(new Set());

  const socketRef = useRef<Socket | null>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const isTypingRef = useRef(false);

  // Scroll to bottom
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Ephemeral countdowns
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      const updates: Record<number, number> = {};
      messages.forEach(m => {
        if (m.isEphemeral && m.ephemeralExpiresAt) {
          const remaining = Math.max(0, Math.ceil((new Date(m.ephemeralExpiresAt).getTime() - now) / 1000));
          updates[m.id] = remaining;
        }
      });
      setEphemeralCountdowns(updates);
    }, 1000);
    return () => clearInterval(interval);
  }, [messages]);

  const initSocket = useCallback((user: User) => {
    const socket = io({ path: '/socket.io' });
    socketRef.current = socket;

    socket.on('connect', () => {
      socket.emit('user:identify', user.id);
    });

    socket.on('message:new', (msg: Message) => {
      setMessages(prev => {
        if (prev.find(m => m.id === msg.id)) return prev;
        return [...prev, msg];
      });
      // Refresh unread for all rooms
      setUnreadCounts(prev => {
        const roomId = msg.roomId;
        // Will be refreshed properly below
        return { ...prev, [roomId]: (prev[roomId] ?? 0) + 1 };
      });
    });

    socket.on('message:edited', (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? msg : m));
    });

    socket.on('message:deleted', ({ messageId }: { messageId: number }) => {
      setMessages(prev => prev.filter(m => m.id !== messageId));
    });

    socket.on('message:read', ({ userId, messageIds }: { userId: number; messageIds: number[]; roomId: number }) => {
      setReadReceipts(prev => {
        const next = { ...prev };
        for (const mid of messageIds) {
          if (!next[mid]) next[mid] = [];
          if (!next[mid].includes(userId)) next[mid] = [...next[mid], userId];
        }
        return next;
      });
    });

    socket.on('reaction:updated', ({ messageId, reactions: reacts }: { messageId: number; reactions: Reaction[] }) => {
      setReactions(prev => ({ ...prev, [messageId]: reacts }));
    });

    socket.on('typing:update', ({ userId, username, typing: isTyping }: { userId: number; username: string; typing: boolean }) => {
      setTyping(prev => {
        const next = { ...prev };
        if (isTyping) {
          if (next[userId]?.timeout) clearTimeout(next[userId].timeout);
          const timeout = setTimeout(() => {
            setTyping(p => { const n = { ...p }; delete n[userId]; return n; });
          }, 5000);
          next[userId] = { username, timeout };
        } else {
          if (next[userId]?.timeout) clearTimeout(next[userId].timeout);
          delete next[userId];
        }
        return next;
      });
    });

    socket.on('user:online', ({ userId, isOnline }: { userId: number; isOnline: boolean }) => {
      setUsers(prev => prev.map(u => u.id === userId ? { ...u, isOnline } : u));
    });

    socket.on('user:status', ({ userId, status, isOnline, lastActive }: { userId: number; status: string; isOnline: boolean; lastActive: string }) => {
      setUsers(prev => prev.map(u => u.id === userId ? { ...u, status, isOnline, lastActive } : u));
    });

    socket.on('room:created', (room: Room) => {
      setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
    });

    socket.on('room:kicked', ({ roomId }: { roomId: number }) => {
      if (currentRoom?.id === roomId) {
        setCurrentRoom(null);
        setMessages([]);
      }
      setJoinedRooms(prev => { const s = new Set(prev); s.delete(roomId); return s; });
    });

    socket.on('room:banned', ({ roomId }: { roomId: number }) => {
      if (currentRoom?.id === roomId) {
        setCurrentRoom(null);
        setMessages([]);
      }
      setJoinedRooms(prev => { const s = new Set(prev); s.delete(roomId); return s; });
    });

    socket.on('room:promoted', ({ userId: promotedId }: { roomId: number; userId: number }) => {
      setRoomMembers(prev => prev.map(m => m.userId === promotedId ? { ...m, isAdmin: true } : m));
    });

    socket.on('scheduled:new', (sched: ScheduledMessage) => {
      if (sched.userId === user.id) {
        setScheduledMsgs(prev => [...prev, sched]);
      }
    });

    socket.on('scheduled:cancelled', ({ id }: { id: number }) => {
      setScheduledMsgs(prev => prev.filter(s => s.id !== id));
    });

    socket.on('scheduled:sent', ({ id }: { id: number }) => {
      setScheduledMsgs(prev => prev.filter(s => s.id !== id));
    });

    return socket;
  }, [currentRoom]);

  // Activity tracking
  useEffect(() => {
    if (!currentUser || !socketRef.current) return;
    const onActivity = () => {
      socketRef.current?.emit('user:activity', currentUser.id);
    };
    window.addEventListener('mousemove', onActivity);
    window.addEventListener('keydown', onActivity);
    return () => {
      window.removeEventListener('mousemove', onActivity);
      window.removeEventListener('keydown', onActivity);
    };
  }, [currentUser]);

  const loadRooms = useCallback(async () => {
    const res = await fetch('/api/rooms');
    const data = await res.json();
    setRooms(data);
  }, []);

  const loadUsers = useCallback(async () => {
    const res = await fetch('/api/users');
    const data = await res.json();
    setUsers(data);
  }, []);

  const loadUnreadCounts = useCallback(async (userId: number, roomList: Room[]) => {
    const counts: Record<number, number> = {};
    await Promise.all(roomList.map(async (r) => {
      const res = await fetch(`/api/rooms/${r.id}/unread/${userId}`);
      const data = await res.json();
      counts[r.id] = data.unread;
    }));
    setUnreadCounts(counts);
  }, []);

  const loadRoomData = useCallback(async (room: Room, userId: number) => {
    // Messages
    const res = await fetch(`/api/rooms/${room.id}/messages`);
    const msgs: Message[] = await res.json();
    setMessages(msgs);

    // Members
    const mRes = await fetch(`/api/rooms/${room.id}/members`);
    const members: RoomMember[] = await mRes.json();
    setRoomMembers(members);

    // Reactions for all messages
    const reactionMap: Record<number, Reaction[]> = {};
    await Promise.all(msgs.map(async (m) => {
      const rRes = await fetch(`/api/messages/${m.id}/reactions`);
      const rData: Reaction[] = await rRes.json();
      if (rData.length > 0) reactionMap[m.id] = rData;
    }));
    setReactions(reactionMap);

    // Read receipts for all messages
    const receiptMap: Record<number, number[]> = {};
    await Promise.all(msgs.map(async (m) => {
      const rrRes = await fetch(`/api/messages/${m.id}/receipts`);
      const rrData: ReadReceipt[] = await rrRes.json();
      if (rrData.length > 0) receiptMap[m.id] = rrData.map(r => r.userId);
    }));
    setReadReceipts(receiptMap);

    // Scheduled messages for this user in this room
    const sRes = await fetch(`/api/rooms/${room.id}/scheduled/${userId}`);
    const sData: ScheduledMessage[] = await sRes.json();
    setScheduledMsgs(sData);

    // Mark all messages as read
    if (msgs.length > 0) {
      const ids = msgs.map(m => m.id);
      await fetch(`/api/rooms/${room.id}/read`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId, messageIds: ids }),
      });
      setUnreadCounts(prev => ({ ...prev, [room.id]: 0 }));
    }
  }, []);

  const handleRegister = async () => {
    if (!usernameInput.trim()) return;
    const res = await fetch('/api/users/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: usernameInput.trim() }),
    });
    const user = await res.json();
    if (user.error) { alert(user.error); return; }
    setCurrentUser(user);
    const socket = initSocket(user);
    await loadRooms();
    const userRes = await fetch('/api/users');
    const allUsers = await userRes.json();
    setUsers(allUsers);
    await loadUnreadCounts(user.id, await (await fetch('/api/rooms')).json());
    // Join all rooms
    const roomsData: Room[] = await (await fetch('/api/rooms')).json();
    for (const r of roomsData) {
      socket.emit('room:join', r.id);
    }
  };

  const handleCreateRoom = async () => {
    if (!newRoomName.trim() || !currentUser) return;
    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });
    const room = await res.json();
    if (room.error) { alert(room.error); return; }
    setNewRoomName('');
    setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
    setJoinedRooms(prev => new Set([...prev, room.id]));
    socketRef.current?.emit('room:join', room.id);
    await loadUnreadCounts(currentUser.id, [...rooms, room]);
  };

  const handleJoinRoom = async (room: Room) => {
    if (!currentUser) return;
    if (!joinedRooms.has(room.id)) {
      const res = await fetch(`/api/rooms/${room.id}/join`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      const data = await res.json();
      if (data.error) { alert(data.error); return; }
      setJoinedRooms(prev => new Set([...prev, room.id]));
      socketRef.current?.emit('room:join', room.id);
    }
    setCurrentRoom(room);
    setTyping({});
    await loadRoomData(room, currentUser.id);
    setUnreadCounts(prev => ({ ...prev, [room.id]: 0 }));
  };

  const handleSendMessage = async () => {
    if (!messageInput.trim() || !currentRoom || !currentUser) return;
    const res = await fetch(`/api/rooms/${currentRoom.id}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        userId: currentUser.id,
        content: messageInput.trim(),
        isEphemeral,
        ephemeralMinutes: isEphemeral ? ephemeralMinutes : undefined,
      }),
    });
    const msg = await res.json();
    if (msg.error) { alert(msg.error); return; }
    setMessageInput('');
    setIsEphemeral(false);
    stopTyping();
  };

  const stopTyping = () => {
    if (isTypingRef.current && currentRoom && currentUser) {
      socketRef.current?.emit('typing:stop', { roomId: currentRoom.id, userId: currentUser.id });
      isTypingRef.current = false;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
  };

  const handleTyping = () => {
    if (!currentRoom || !currentUser) return;
    if (!isTypingRef.current) {
      socketRef.current?.emit('typing:start', { roomId: currentRoom.id, userId: currentUser.id, username: currentUser.username });
      isTypingRef.current = true;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(stopTyping, 3000);
  };

  const handleEditMessage = async (msgId: number) => {
    if (!currentUser || !editContent.trim()) return;
    await fetch(`/api/messages/${msgId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: editContent.trim() }),
    });
    setEditingMsgId(null);
    setEditContent('');
  };

  const handleReaction = async (msgId: number, emoji: string) => {
    if (!currentUser) return;
    await fetch(`/api/messages/${msgId}/reactions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
  };

  const handleSchedule = async () => {
    if (!scheduleContent.trim() || !scheduleTime || !currentRoom || !currentUser) return;
    const res = await fetch(`/api/rooms/${currentRoom.id}/schedule`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: scheduleContent.trim(), scheduledFor: scheduleTime }),
    });
    const data = await res.json();
    if (data.error) { alert(data.error); return; }
    setScheduleContent('');
    setScheduleTime('');
    setScheduledMsgs(prev => [...prev.filter(s => s.id !== data.id), data]);
  };

  const handleCancelScheduled = async (id: number) => {
    if (!currentUser) return;
    await fetch(`/api/scheduled/${id}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setScheduledMsgs(prev => prev.filter(s => s.id !== id));
  };

  const handleViewHistory = async (msgId: number) => {
    const res = await fetch(`/api/messages/${msgId}/history`);
    const data = await res.json();
    setEditHistory(data);
  };

  const handleSetStatus = async (status: string) => {
    if (!currentUser) return;
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
    setCurrentUser(prev => prev ? { ...prev, status } : prev);
  };

  const handleKick = async (targetUserId: number) => {
    if (!currentUser || !currentRoom) return;
    await fetch(`/api/rooms/${currentRoom.id}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    setRoomMembers(prev => prev.filter(m => m.userId !== targetUserId));
  };

  const handleBan = async (targetUserId: number) => {
    if (!currentUser || !currentRoom) return;
    await fetch(`/api/rooms/${currentRoom.id}/ban`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    setRoomMembers(prev => prev.filter(m => m.userId !== targetUserId));
  };

  const handlePromote = async (targetUserId: number) => {
    if (!currentUser || !currentRoom) return;
    await fetch(`/api/rooms/${currentRoom.id}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
  };

  const markRoomRead = useCallback(async (roomId: number, msgs: Message[]) => {
    if (!currentUser || msgs.length === 0) return;
    const ids = msgs.map(m => m.id);
    await fetch(`/api/rooms/${roomId}/read`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, messageIds: ids }),
    });
    setUnreadCounts(prev => ({ ...prev, [roomId]: 0 }));
  }, [currentUser]);

  // When new messages arrive in current room, mark read
  useEffect(() => {
    if (currentRoom && messages.length > 0 && currentUser) {
      markRoomRead(currentRoom.id, messages);
    }
  }, [messages, currentRoom, currentUser, markRoomRead]);

  // Refresh users periodically
  useEffect(() => {
    if (!currentUser) return;
    const interval = setInterval(loadUsers, 30000);
    return () => clearInterval(interval);
  }, [currentUser, loadUsers]);

  const typingUsers = Object.values(typing).filter((_, i) => {
    const uid = parseInt(Object.keys(typing)[i]);
    return uid !== currentUser?.id;
  });

  const currentMember = roomMembers.find(m => m.userId === currentUser?.id);
  const isAdmin = currentMember?.isAdmin ?? false;

  function getStatusColor(status: string, isOnline?: boolean): string {
    if (!isOnline) return '#888';
    switch (status) {
      case 'online': return '#27AE60';
      case 'away': return '#F39C12';
      case 'dnd': return '#E74C3C';
      case 'invisible': return '#888';
      default: return '#27AE60';
    }
  }

  function getStatusLabel(status: string): string {
    switch (status) {
      case 'online': return 'Online';
      case 'away': return 'Away';
      case 'dnd': return 'Do Not Disturb';
      case 'invisible': return 'Invisible';
      default: return status;
    }
  }

  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-box">
          <h1>PostgreSQL Chat</h1>
          <p>Enter your username to start chatting</p>
          <div className="login-form">
            <input
              type="text"
              placeholder="Your username"
              value={usernameInput}
              onChange={e => setUsernameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleRegister()}
              maxLength={32}
            />
            <button onClick={handleRegister} className="btn-primary">Join</button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>PostgreSQL Chat</h2>
          <div className="user-info">
            <span
              className="status-dot"
              style={{ backgroundColor: getStatusColor(currentUser.status, true) }}
              title={getStatusLabel(currentUser.status)}
            />
            <span className="username">{currentUser.username}</span>
          </div>
          <div className="status-selector">
            {['online', 'away', 'dnd', 'invisible'].map(s => (
              <button
                key={s}
                className={`status-btn ${currentUser.status === s ? 'active' : ''}`}
                onClick={() => handleSetStatus(s)}
                title={getStatusLabel(s)}
                style={{ borderColor: getStatusColor(s, true) }}
              >
                {getStatusLabel(s)}
              </button>
            ))}
          </div>
        </div>

        <div className="sidebar-section">
          <h3>Rooms</h3>
          <div className="room-list">
            {rooms.map(room => (
              <div
                key={room.id}
                className={`room-item ${currentRoom?.id === room.id ? 'active' : ''}`}
                onClick={() => handleJoinRoom(room)}
              >
                <span># {room.name}</span>
                {(unreadCounts[room.id] ?? 0) > 0 && (
                  <span className="badge">{unreadCounts[room.id]}</span>
                )}
              </div>
            ))}
          </div>
          <div className="create-room">
            <input
              type="text"
              placeholder="New room name"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
              maxLength={64}
            />
            <button onClick={handleCreateRoom} className="btn-primary">+</button>
          </div>
        </div>

        <div className="sidebar-section">
          <h3>Online Users</h3>
          <div className="user-list">
            {users.map(u => (
              <div key={u.id} className="user-item">
                <span
                  className="status-dot"
                  style={{ backgroundColor: getStatusColor(u.status, u.isOnline) }}
                  title={u.isOnline ? getStatusLabel(u.status) : `Last active ${timeAgo(u.lastActive)}`}
                />
                <span>{u.username}</span>
                {u.id === currentUser.id && <span className="you-tag"> (you)</span>}
                {!u.isOnline && <span className="last-active"> · {timeAgo(u.lastActive)}</span>}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Main chat area */}
      <div className="main">
        {currentRoom ? (
          <>
            <div className="room-header">
              <h2># {currentRoom.name}</h2>
              <div className="room-actions">
                <button onClick={() => setShowScheduled(!showScheduled)} className="btn-secondary">
                  {showScheduled ? 'Hide Scheduled' : 'Scheduled'}
                </button>
                <button onClick={() => setShowMembers(!showMembers)} className="btn-secondary">
                  Members
                </button>
              </div>
            </div>

            {showMembers && (
              <div className="members-panel">
                <h4>Room Members</h4>
                {roomMembers.filter(m => !m.isBanned).map(m => {
                  const u = users.find(u => u.id === m.userId);
                  return (
                    <div key={m.id} className="member-item">
                      <span>{u?.username ?? `User ${m.userId}`}</span>
                      {m.isAdmin && <span className="admin-badge">Admin</span>}
                      {isAdmin && m.userId !== currentUser.id && !m.isAdmin && (
                        <div className="member-actions">
                          <button onClick={() => handlePromote(m.userId)} className="btn-tiny">Promote</button>
                          <button onClick={() => handleKick(m.userId)} className="btn-tiny danger">Kick</button>
                          <button onClick={() => handleBan(m.userId)} className="btn-tiny danger">Ban</button>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {showScheduled && (
              <div className="scheduled-panel">
                <h4>Schedule a Message</h4>
                <div className="schedule-form">
                  <input
                    type="text"
                    placeholder="Message content"
                    value={scheduleContent}
                    onChange={e => setScheduleContent(e.target.value)}
                  />
                  <input
                    type="datetime-local"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                  />
                  <button onClick={handleSchedule} className="btn-primary">Schedule</button>
                </div>
                {scheduledMsgs.length > 0 && (
                  <div className="scheduled-list">
                    <h5>Pending Scheduled Messages</h5>
                    {scheduledMsgs.map(s => (
                      <div key={s.id} className="scheduled-item">
                        <span className="scheduled-time">{new Date(s.scheduledFor).toLocaleString()}</span>
                        <span className="scheduled-content">{s.content}</span>
                        <button onClick={() => handleCancelScheduled(s.id)} className="btn-tiny danger">Cancel</button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            <div className="messages">
              {messages.map(msg => {
                const sender = users.find(u => u.id === msg.userId);
                const msgReactions = reactions[msg.id] ?? [];
                const msgReceipts = readReceipts[msg.id] ?? [];
                const seenBy = msgReceipts
                  .filter(uid => uid !== msg.userId)
                  .map(uid => users.find(u => u.id === uid)?.username)
                  .filter(Boolean);
                const countdown = ephemeralCountdowns[msg.id];
                const isOwn = msg.userId === currentUser.id;

                // Group reactions by emoji
                const reactionGroups: Record<string, number[]> = {};
                for (const r of msgReactions) {
                  if (!reactionGroups[r.emoji]) reactionGroups[r.emoji] = [];
                  reactionGroups[r.emoji].push(r.userId);
                }

                return (
                  <div key={msg.id} className={`message ${isOwn ? 'own' : ''} ${msg.isEphemeral ? 'ephemeral' : ''}`}>
                    <div className="message-header">
                      <span className="message-author">{sender?.username ?? 'Unknown'}</span>
                      <span className="message-time">{formatTime(msg.createdAt)}</span>
                      {msg.isEphemeral && countdown !== undefined && (
                        <span className="ephemeral-countdown">⏳ {countdown}s</span>
                      )}
                      {msg.isEdited && (
                        <span
                          className="edited-indicator"
                          onClick={() => handleViewHistory(msg.id)}
                          title="View edit history"
                        >(edited)</span>
                      )}
                    </div>

                    {editingMsgId === msg.id ? (
                      <div className="edit-form">
                        <input
                          value={editContent}
                          onChange={e => setEditContent(e.target.value)}
                          onKeyDown={e => {
                            if (e.key === 'Enter') handleEditMessage(msg.id);
                            if (e.key === 'Escape') setEditingMsgId(null);
                          }}
                          autoFocus
                        />
                        <button onClick={() => handleEditMessage(msg.id)} className="btn-tiny">Save</button>
                        <button onClick={() => setEditingMsgId(null)} className="btn-tiny">Cancel</button>
                      </div>
                    ) : (
                      <div className="message-content">{msg.content}</div>
                    )}

                    <div className="message-footer">
                      <div className="reaction-row">
                        {Object.entries(reactionGroups).map(([emoji, uids]) => (
                          <button
                            key={emoji}
                            className={`reaction-chip ${uids.includes(currentUser.id) ? 'reacted' : ''}`}
                            onClick={() => handleReaction(msg.id, emoji)}
                            onMouseEnter={() => setReactionHover(msg.id)}
                            onMouseLeave={() => setReactionHover(null)}
                            title={uids.map(uid => users.find(u => u.id === uid)?.username).join(', ')}
                          >
                            {emoji} {uids.length}
                          </button>
                        ))}
                        <div className="emoji-picker">
                          {EMOJIS.map(e => (
                            <button key={e} className="emoji-btn" onClick={() => handleReaction(msg.id, e)} title={`React with ${e}`}>
                              {e}
                            </button>
                          ))}
                        </div>
                      </div>

                      {isOwn && !editingMsgId && (
                        <button
                          className="btn-tiny edit-btn"
                          onClick={() => { setEditingMsgId(msg.id); setEditContent(msg.content); }}
                        >
                          Edit
                        </button>
                      )}

                      {seenBy.length > 0 && (
                        <div className="seen-by">Seen by {seenBy.join(', ')}</div>
                      )}
                    </div>
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            <div className="typing-indicator">
              {typingUsers.length === 1 && (
                <span>{typingUsers[0].username} is typing...</span>
              )}
              {typingUsers.length === 2 && (
                <span>{typingUsers[0].username} and {typingUsers[1].username} are typing...</span>
              )}
              {typingUsers.length > 2 && (
                <span>Multiple users are typing...</span>
              )}
            </div>

            <div className="input-area">
              <div className="input-options">
                <label className="ephemeral-toggle">
                  <input
                    type="checkbox"
                    checked={isEphemeral}
                    onChange={e => setIsEphemeral(e.target.checked)}
                  />
                  Ephemeral
                </label>
                {isEphemeral && (
                  <select value={ephemeralMinutes} onChange={e => setEphemeralMinutes(parseInt(e.target.value))}>
                    <option value={1}>1 min</option>
                    <option value={5}>5 min</option>
                    <option value={10}>10 min</option>
                  </select>
                )}
              </div>
              <div className="input-row">
                <input
                  type="text"
                  placeholder={`Message #${currentRoom.name}`}
                  value={messageInput}
                  onChange={e => { setMessageInput(e.target.value); handleTyping(); }}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSendMessage();
                    }
                  }}
                />
                <button onClick={handleSendMessage} className="btn-primary">Send</button>
              </div>
            </div>
          </>
        ) : (
          <div className="no-room">
            <h2>PostgreSQL Chat</h2>
            <p>Select a room to start chatting</p>
          </div>
        )}
      </div>

      {/* Edit history modal */}
      {editHistory && (
        <div className="modal-overlay" onClick={() => setEditHistory(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Edit History</h3>
            {editHistory.length === 0 ? (
              <p>No edit history</p>
            ) : (
              editHistory.map(e => (
                <div key={e.id} className="edit-history-item">
                  <div className="edit-time">{new Date(e.editedAt).toLocaleString()}</div>
                  <div className="edit-old"><strong>Before:</strong> {e.oldContent}</div>
                  <div className="edit-new"><strong>After:</strong> {e.newContent}</div>
                </div>
              ))
            )}
            <button onClick={() => setEditHistory(null)} className="btn-secondary">Close</button>
          </div>
        </div>
      )}
    </div>
  );
}
