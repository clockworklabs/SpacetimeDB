import React, {
  useState,
  useEffect,
  useRef,
  useCallback,
} from 'react';
import { io, Socket } from 'socket.io-client';

// ─── Types ───────────────────────────────────────────────────────────────────

interface User {
  id: number;
  username: string;
  status: 'online' | 'away' | 'dnd' | 'invisible';
  lastActive: string;
}

interface Room {
  id: number;
  name: string;
  creatorId: number;
  createdAt: string;
}

interface ReactionGroup {
  emoji: string;
  count: number;
  users: string[];
  userIds: number[];
}

interface MessageReader {
  userId: number;
  username: string;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  username: string;
  content: string;
  createdAt: string;
  expiresAt: string | null;
  scheduledAt: string | null;
  isSent: boolean;
  isDeleted: boolean;
  editedAt: string | null;
  reactions: ReactionGroup[];
  readers: MessageReader[];
}

interface Member {
  userId: number;
  username: string;
  isAdmin: boolean;
  isBanned: boolean;
  joinedAt: string;
  status: string;
  lastActive: string;
}

interface ScheduledMsg {
  id: number;
  content: string;
  scheduledAt: string;
  createdAt: string;
}

interface EditRecord {
  id: number;
  messageId: number;
  oldContent: string;
  editedAt: string;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatTime(ts: string) {
  return new Date(ts).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatRelative(ts: string) {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

function statusColor(status: string) {
  switch (status) {
    case 'online': return '#27ae60';
    case 'away': return '#f26522';
    case 'dnd': return '#cc3b03';
    case 'invisible': return '#848484';
    default: return '#848484';
  }
}

function statusLabel(status: string) {
  switch (status) {
    case 'online': return 'Online';
    case 'away': return 'Away';
    case 'dnd': return 'Do Not Disturb';
    case 'invisible': return 'Invisible';
    default: return status;
  }
}

function secondsRemaining(expiresAt: string | null): number {
  if (!expiresAt) return Infinity;
  return Math.max(0, Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000));
}

const EMOJI_LIST = ['👍', '❤️', '😂', '😮', '😢', '🎉'];

// ─── Socket singleton ─────────────────────────────────────────────────────────

let socketInstance: Socket | null = null;
function getSocket(): Socket {
  if (!socketInstance) {
    socketInstance = io('/', { autoConnect: false });
  }
  return socketInstance;
}

// ─── App Component ────────────────────────────────────────────────────────────

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [usernameInput, setUsernameInput] = useState('');
  const [loginError, setLoginError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [joinedRooms, setJoinedRooms] = useState<Set<number>>(new Set());
  const [currentRoomId, setCurrentRoomId] = useState<number | null>(null);
  const [newRoomName, setNewRoomName] = useState('');

  const [messages, setMessages] = useState<Message[]>([]);
  const [members, setMembers] = useState<Member[]>([]);
  const [scheduledMsgs, setScheduledMsgs] = useState<ScheduledMsg[]>([]);

  const [msgInput, setMsgInput] = useState('');
  const [typingUsers, setTypingUsers] = useState<string[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});

  // Ephemeral / schedule options
  const [ephemeralSecs, setEphemeralSecs] = useState<number | ''>('');
  const [scheduleAt, setScheduleAt] = useState('');
  const [showMsgOptions, setShowMsgOptions] = useState(false);

  // Editing
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editContent, setEditContent] = useState('');

  // Edit history modal
  const [historyMsgId, setHistoryMsgId] = useState<number | null>(null);
  const [historyRecords, setHistoryRecords] = useState<EditRecord[]>([]);

  // Members panel
  const [showMembers, setShowMembers] = useState(false);

  // Reaction picker
  const [reactionPickerMsgId, setReactionPickerMsgId] = useState<number | null>(null);

  // Hover tooltip for reactions
  const [reactionTooltip, setReactionTooltip] = useState<{
    msgId: number;
    emoji: string;
  } | null>(null);

  // Tick for countdowns
  const [tick, setTick] = useState(0);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const socket = getSocket();

  // Tick every second for countdown display
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // Activity tracking - update lastActive every 2 minutes
  useEffect(() => {
    if (!currentUser) return;
    const interval = setInterval(() => {
      socket.emit('update-activity', { userId: currentUser.id });
    }, 120000);
    return () => clearInterval(interval);
  }, [currentUser, socket]);

  // Socket setup
  useEffect(() => {
    if (!currentUser) return;

    socket.connect();
    socket.emit('register', { userId: currentUser.id });

    socket.on('new-message', (msg: Message) => {
      if (msg.roomId === currentRoomId) {
        setMessages((prev) => [...prev, msg]);
      } else {
        setUnreadCounts((prev) => ({
          ...prev,
          [msg.roomId]: (prev[msg.roomId] || 0) + 1,
        }));
      }
    });

    socket.on('message-deleted', ({ messageId }: { messageId: number }) => {
      setMessages((prev) => prev.filter((m) => m.id !== messageId));
    });

    socket.on('message-edited', (updated: Message) => {
      setMessages((prev) =>
        prev.map((m) => (m.id === updated.id ? updated : m)),
      );
    });

    socket.on(
      'reaction-updated',
      ({
        messageId,
        reactions,
      }: {
        messageId: number;
        reactions: ReactionGroup[];
      }) => {
        setMessages((prev) =>
          prev.map((m) => (m.id === messageId ? { ...m, reactions } : m)),
        );
      },
    );

    socket.on(
      'message-read',
      ({
        messageId,
        userId,
        username,
      }: {
        messageId: number;
        userId: number;
        username: string;
      }) => {
        setMessages((prev) =>
          prev.map((m) => {
            if (m.id !== messageId) return m;
            const alreadyRead = m.readers.some((r) => r.userId === userId);
            if (alreadyRead) return m;
            return { ...m, readers: [...m.readers, { userId, username }] };
          }),
        );
      },
    );

    socket.on(
      'typing-update',
      ({ roomId, typingUsers: tu }: { roomId: number; typingUsers: string[] }) => {
        if (roomId === currentRoomId) {
          setTypingUsers(tu.filter((u) => u !== currentUser.username));
        }
      },
    );

    socket.on('room-created', (room: Room) => {
      setRooms((prev) => {
        if (prev.find((r) => r.id === room.id)) return prev;
        return [...prev, room];
      });
    });

    socket.on(
      'member-joined',
      ({ roomId, userId, username }: { roomId: number; userId: number; username: string }) => {
        if (roomId === currentRoomId) {
          setMembers((prev) => {
            if (prev.find((m) => m.userId === userId)) return prev;
            return [
              ...prev,
              {
                userId,
                username,
                isAdmin: false,
                isBanned: false,
                joinedAt: new Date().toISOString(),
                status: 'online',
                lastActive: new Date().toISOString(),
              },
            ];
          });
        }
      },
    );

    socket.on('member-left', ({ roomId, userId }: { roomId: number; userId: number }) => {
      if (roomId === currentRoomId) {
        setMembers((prev) => prev.filter((m) => m.userId !== userId));
      }
    });

    socket.on(
      'user-kicked',
      ({ roomId, userId }: { roomId: number; userId: number }) => {
        if (roomId === currentRoomId) {
          setMembers((prev) => prev.filter((m) => m.userId !== userId));
        }
      },
    );

    socket.on(
      'user-banned',
      ({ roomId, userId }: { roomId: number; userId: number }) => {
        if (roomId === currentRoomId) {
          setMembers((prev) =>
            prev.map((m) => (m.userId === userId ? { ...m, isBanned: true } : m)),
          );
        }
      },
    );

    socket.on('you-were-kicked', ({ roomId }: { roomId: number }) => {
      setJoinedRooms((prev) => {
        const next = new Set(prev);
        next.delete(roomId);
        return next;
      });
      if (currentRoomId === roomId) {
        setCurrentRoomId(null);
        setMessages([]);
        alert('You were kicked from this room.');
      }
    });

    socket.on('you-were-banned', ({ roomId }: { roomId: number }) => {
      setJoinedRooms((prev) => {
        const next = new Set(prev);
        next.delete(roomId);
        return next;
      });
      if (currentRoomId === roomId) {
        setCurrentRoomId(null);
        setMessages([]);
        alert('You were banned from this room.');
      }
    });

    socket.on(
      'member-promoted',
      ({ roomId, userId }: { roomId: number; userId: number }) => {
        if (roomId === currentRoomId) {
          setMembers((prev) =>
            prev.map((m) => (m.userId === userId ? { ...m, isAdmin: true } : m)),
          );
        }
      },
    );

    socket.on(
      'status-changed',
      ({
        userId,
        status,
        lastActive,
      }: {
        userId: number;
        status: string;
        lastActive: string;
      }) => {
        if (currentUser.id === userId) {
          setCurrentUser((prev) =>
            prev ? { ...prev, status: status as User['status'], lastActive } : prev,
          );
        }
        setMembers((prev) =>
          prev.map((m) =>
            m.userId === userId ? { ...m, status, lastActive } : m,
          ),
        );
      },
    );

    socket.on(
      'user-online',
      ({ userId, username }: { userId: number; username: string }) => {
        setMembers((prev) =>
          prev.map((m) =>
            m.userId === userId ? { ...m, status: 'online' } : m,
          ),
        );
        void username;
      },
    );

    socket.on(
      'user-offline',
      ({
        userId,
        lastActive,
      }: {
        userId: number;
        username: string;
        lastActive: string;
      }) => {
        setMembers((prev) =>
          prev.map((m) =>
            m.userId === userId ? { ...m, status: 'away', lastActive } : m,
          ),
        );
      },
    );

    return () => {
      socket.off('new-message');
      socket.off('message-deleted');
      socket.off('message-edited');
      socket.off('reaction-updated');
      socket.off('message-read');
      socket.off('typing-update');
      socket.off('room-created');
      socket.off('member-joined');
      socket.off('member-left');
      socket.off('user-kicked');
      socket.off('user-banned');
      socket.off('you-were-kicked');
      socket.off('you-were-banned');
      socket.off('member-promoted');
      socket.off('status-changed');
      socket.off('user-online');
      socket.off('user-offline');
      socket.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentUser?.id]);

  // Update typing listener when room changes
  useEffect(() => {
    socket.off('typing-update');
    socket.on(
      'typing-update',
      ({ roomId, typingUsers: tu }: { roomId: number; typingUsers: string[] }) => {
        if (roomId === currentRoomId) {
          setTypingUsers(tu.filter((u) => u !== currentUser?.username));
        }
      },
    );
  }, [currentRoomId, currentUser?.username, socket]);

  // Fetch rooms on login
  useEffect(() => {
    if (!currentUser) return;
    fetchRooms();
  }, [currentUser]);

  // Load room when changed
  useEffect(() => {
    if (!currentUser || !currentRoomId) return;
    loadRoom(currentRoomId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentRoomId]);

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Mark messages as read when room opens
  useEffect(() => {
    if (!currentUser || !currentRoomId || messages.length === 0) return;
    const unread = messages.filter(
      (m) => !m.readers.some((r) => r.userId === currentUser.id),
    );
    if (unread.length === 0) return;

    fetch(`/api/rooms/${currentRoomId}/read`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        userId: currentUser.id,
        messageIds: unread.map((m) => m.id),
      }),
    }).catch(console.error);

    setUnreadCounts((prev) => ({ ...prev, [currentRoomId]: 0 }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentRoomId, messages.length]);

  // ─── Data fetchers ──────────────────────────────────────────────────────────

  async function fetchRooms() {
    const res = await fetch('/api/rooms');
    const data: Room[] = await res.json();
    setRooms(data);

    if (!currentUser) return;
    // Fetch unread counts for all rooms
    for (const room of data) {
      fetch(`/api/rooms/${room.id}/unread?userId=${currentUser.id}`)
        .then((r) => r.json())
        .then(({ count }: { count: number }) => {
          if (count > 0) {
            setUnreadCounts((prev) => ({ ...prev, [room.id]: count }));
          }
        })
        .catch(console.error);
    }
  }

  async function loadRoom(roomId: number) {
    if (!currentUser) return;
    socket.emit('join-room', { roomId });

    const [msgsRes, membersRes, scheduledRes] = await Promise.all([
      fetch(`/api/rooms/${roomId}/messages?userId=${currentUser.id}`),
      fetch(`/api/rooms/${roomId}/members`),
      fetch(`/api/rooms/${roomId}/messages/scheduled?userId=${currentUser.id}`),
    ]);

    const [msgs, mems, sched]: [Message[], Member[], ScheduledMsg[]] =
      await Promise.all([msgsRes.json(), membersRes.json(), scheduledRes.json()]);

    setMessages(msgs);
    setMembers(mems);
    setScheduledMsgs(sched);
    setTypingUsers([]);
    setUnreadCounts((prev) => ({ ...prev, [roomId]: 0 }));
  }

  // ─── Actions ────────────────────────────────────────────────────────────────

  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    setLoginError('');
    const name = usernameInput.trim();
    if (!name) return;

    const res = await fetch('/api/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username: name }),
    });

    if (!res.ok) {
      const err = await res.json();
      setLoginError(err.error || 'Failed to login');
      return;
    }

    const user: User = await res.json();
    setCurrentUser(user);
  }

  async function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    if (!currentUser || !newRoomName.trim()) return;

    const res = await fetch('/api/rooms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id }),
    });

    if (res.ok) {
      const room: Room = await res.json();
      setNewRoomName('');
      setJoinedRooms((prev) => new Set([...prev, room.id]));
      setCurrentRoomId(room.id);
    } else {
      const err = await res.json();
      alert(err.error || 'Failed to create room');
    }
  }

  async function handleJoinRoom(roomId: number) {
    if (!currentUser) return;

    const res = await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });

    if (res.ok) {
      setJoinedRooms((prev) => new Set([...prev, roomId]));
      setCurrentRoomId(roomId);
    } else {
      const err = await res.json();
      alert(err.error || 'Failed to join room');
    }
  }

  function handleSelectRoom(roomId: number) {
    if (!joinedRooms.has(roomId)) {
      handleJoinRoom(roomId);
    } else {
      setCurrentRoomId(roomId);
    }
  }

  const handleTyping = useCallback(() => {
    if (!currentUser || !currentRoomId) return;
    socket.emit('typing', {
      roomId: currentRoomId,
      userId: currentUser.id,
      username: currentUser.username,
    });

    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      socket.emit('stop-typing', {
        roomId: currentRoomId,
        userId: currentUser.id,
      });
    }, 2500);
  }, [currentUser, currentRoomId, socket]);

  async function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!currentUser || !currentRoomId || !msgInput.trim()) return;

    const body: Record<string, unknown> = {
      userId: currentUser.id,
      content: msgInput.trim(),
    };

    if (ephemeralSecs) body.expiresInSeconds = Number(ephemeralSecs);
    if (scheduleAt) body.scheduledAt = scheduleAt;

    const res = await fetch(`/api/rooms/${currentRoomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (res.ok) {
      const msg: Message = await res.json();
      if (scheduleAt) {
        setScheduledMsgs((prev) => [
          ...prev,
          {
            id: msg.id,
            content: msg.content,
            scheduledAt: msg.scheduledAt!,
            createdAt: msg.createdAt,
          },
        ]);
      }
      setMsgInput('');
      setEphemeralSecs('');
      setScheduleAt('');
      setShowMsgOptions(false);

      // Stop typing indicator
      socket.emit('stop-typing', {
        roomId: currentRoomId,
        userId: currentUser.id,
      });
    } else {
      const err = await res.json();
      alert(err.error || 'Failed to send message');
    }
  }

  async function handleCancelScheduled(msgId: number) {
    if (!currentUser) return;
    const res = await fetch(`/api/messages/${msgId}/scheduled`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    if (res.ok) {
      setScheduledMsgs((prev) => prev.filter((m) => m.id !== msgId));
    }
  }

  async function handleEditSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!currentUser || !editingId || !editContent.trim()) return;

    const res = await fetch(`/api/messages/${editingId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content: editContent }),
    });

    if (res.ok) {
      setEditingId(null);
      setEditContent('');
    } else {
      const err = await res.json();
      alert(err.error || 'Failed to edit message');
    }
  }

  async function handleViewHistory(msgId: number) {
    const res = await fetch(`/api/messages/${msgId}/history`);
    const history: EditRecord[] = await res.json();
    setHistoryRecords(history);
    setHistoryMsgId(msgId);
  }

  async function handleReact(msgId: number, emoji: string) {
    if (!currentUser) return;
    await fetch(`/api/messages/${msgId}/react`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
    setReactionPickerMsgId(null);
  }

  async function handleKick(targetUserId: number) {
    if (!currentUser || !currentRoomId) return;
    if (!confirm('Kick this user?')) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    if (!res.ok) {
      const err = await res.json();
      alert(err.error);
    }
  }

  async function handleBan(targetUserId: number) {
    if (!currentUser || !currentRoomId) return;
    if (!confirm('Ban this user?')) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/ban`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    if (!res.ok) {
      const err = await res.json();
      alert(err.error);
    }
  }

  async function handlePromote(targetUserId: number) {
    if (!currentUser || !currentRoomId) return;
    if (!confirm('Promote this user to admin?')) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    if (!res.ok) {
      const err = await res.json();
      alert(err.error);
    }
  }

  async function handleStatusChange(status: string) {
    if (!currentUser) return;
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
    setCurrentUser((prev) =>
      prev ? { ...prev, status: status as User['status'] } : prev,
    );
  }

  // ─── Render helpers ─────────────────────────────────────────────────────────

  const currentRoom = rooms.find((r) => r.id === currentRoomId);
  const currentMember = members.find((m) => m.userId === currentUser?.id);
  const isAdmin = currentMember?.isAdmin ?? false;

  // ─── Login Screen ────────────────────────────────────────────────────────────

  if (!currentUser) {
    return (
      <div className="login-container">
        <div className="login-card">
          <div className="login-logo">
            <span className="login-elephant">🐘</span>
          </div>
          <h1 className="login-title">PostgreSQL Chat</h1>
          <p className="login-subtitle">Enter a username to get started</p>
          <form onSubmit={handleLogin} className="login-form">
            <input
              className="login-input"
              type="text"
              placeholder="Your username"
              value={usernameInput}
              onChange={(e) => setUsernameInput(e.target.value)}
              maxLength={50}
              autoFocus
            />
            {loginError && <p className="login-error">{loginError}</p>}
            <button className="login-btn" type="submit">
              Enter Chat
            </button>
          </form>
        </div>
      </div>
    );
  }

  // ─── Main App ────────────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* ─── Sidebar ─── */}
      <aside className="sidebar">
        {/* Header */}
        <div className="sidebar-header">
          <span className="sidebar-title">PostgreSQL Chat</span>
          <span className="sidebar-elephant">🐘</span>
        </div>

        {/* Current user + status */}
        <div className="user-info">
          <div className="user-info-row">
            <span
              className="status-dot"
              style={{ background: statusColor(currentUser.status) }}
            />
            <span className="user-name">{currentUser.username}</span>
          </div>
          <select
            className="status-select"
            value={currentUser.status}
            onChange={(e) => handleStatusChange(e.target.value)}
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        {/* Create Room */}
        <div className="section">
          <form onSubmit={handleCreateRoom} className="create-room-form">
            <input
              className="input-sm"
              placeholder="New room name..."
              value={newRoomName}
              onChange={(e) => setNewRoomName(e.target.value)}
              maxLength={100}
            />
            <button className="btn-sm btn-primary" type="submit">
              +
            </button>
          </form>
        </div>

        {/* Room List */}
        <div className="room-list">
          {rooms.map((room) => {
            const unread = unreadCounts[room.id] || 0;
            const isJoined = joinedRooms.has(room.id);
            const isActive = currentRoomId === room.id;
            return (
              <button
                key={room.id}
                className={`room-item ${isActive ? 'room-item--active' : ''} ${!isJoined ? 'room-item--unjoined' : ''}`}
                onClick={() => handleSelectRoom(room.id)}
              >
                <span className="room-item-hash">#</span>
                <span className="room-item-name">{room.name}</span>
                {unread > 0 && (
                  <span className="unread-badge">{unread}</span>
                )}
              </button>
            );
          })}
          {rooms.length === 0 && (
            <p className="empty-state">No rooms yet. Create one!</p>
          )}
        </div>
      </aside>

      {/* ─── Chat Area ─── */}
      <main className="chat-area">
        {currentRoom ? (
          <>
            {/* Chat Header */}
            <div className="chat-header">
              <div className="chat-header-left">
                <span className="chat-room-name">#{currentRoom.name}</span>
                <span className="chat-member-count">
                  {members.filter((m) => !m.isBanned).length} members
                </span>
              </div>
              <button
                className="btn-sm btn-secondary"
                onClick={() => setShowMembers((v) => !v)}
              >
                {showMembers ? 'Hide Members' : 'Members'}
              </button>
            </div>

            {/* Scheduled Messages Banner */}
            {scheduledMsgs.length > 0 && (
              <div className="scheduled-banner">
                <strong>Scheduled ({scheduledMsgs.length}):</strong>
                {scheduledMsgs.map((sm) => (
                  <span key={sm.id} className="scheduled-item">
                    "{sm.content.slice(0, 30)}" at{' '}
                    {new Date(sm.scheduledAt).toLocaleTimeString()}
                    <button
                      className="btn-tiny btn-danger"
                      onClick={() => handleCancelScheduled(sm.id)}
                    >
                      ✕
                    </button>
                  </span>
                ))}
              </div>
            )}

            {/* Messages */}
            <div
              className="messages"
              onClick={() => setReactionPickerMsgId(null)}
            >
              {messages.map((msg) => {
                const isOwn = msg.userId === currentUser.id;
                const secs = secondsRemaining(msg.expiresAt);
                void tick; // use tick for countdown re-render
                const isEphemeral = msg.expiresAt !== null;
                const seenBy = msg.readers.filter(
                  (r) => r.userId !== currentUser.id,
                );

                return (
                  <div
                    key={msg.id}
                    className={`message ${isOwn ? 'message--own' : ''}`}
                  >
                    <div className="message-header">
                      <span className="message-author">{msg.username}</span>
                      <span className="message-time">{formatTime(msg.createdAt)}</span>
                      {isEphemeral && (
                        <span className="ephemeral-badge">
                          ⏳ {secs > 0 ? `${secs}s` : 'expiring...'}
                        </span>
                      )}
                      {msg.editedAt && (
                        <span
                          className="edited-badge"
                          onClick={() => handleViewHistory(msg.id)}
                          title="Click to see edit history"
                        >
                          (edited)
                        </span>
                      )}
                    </div>

                    {editingId === msg.id ? (
                      <form onSubmit={handleEditSubmit} className="edit-form">
                        <input
                          className="input-edit"
                          value={editContent}
                          onChange={(e) => setEditContent(e.target.value)}
                          autoFocus
                        />
                        <button className="btn-tiny btn-primary" type="submit">
                          Save
                        </button>
                        <button
                          className="btn-tiny"
                          type="button"
                          onClick={() => setEditingId(null)}
                        >
                          Cancel
                        </button>
                      </form>
                    ) : (
                      <div className="message-content">{msg.content}</div>
                    )}

                    {/* Reactions */}
                    <div className="reactions-row">
                      {msg.reactions.map((rg) => {
                        const userReacted = rg.userIds.includes(currentUser.id);
                        const showTooltip =
                          reactionTooltip?.msgId === msg.id &&
                          reactionTooltip.emoji === rg.emoji;
                        return (
                          <button
                            key={rg.emoji}
                            className={`reaction-btn ${userReacted ? 'reaction-btn--active' : ''}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              handleReact(msg.id, rg.emoji);
                            }}
                            onMouseEnter={() =>
                              setReactionTooltip({ msgId: msg.id, emoji: rg.emoji })
                            }
                            onMouseLeave={() => setReactionTooltip(null)}
                          >
                            {rg.emoji} {rg.count}
                            {showTooltip && (
                              <span className="reaction-tooltip">
                                {rg.users.join(', ')}
                              </span>
                            )}
                          </button>
                        );
                      })}
                      <button
                        className="btn-tiny reaction-add"
                        onClick={(e) => {
                          e.stopPropagation();
                          setReactionPickerMsgId(
                            reactionPickerMsgId === msg.id ? null : msg.id,
                          );
                        }}
                      >
                        +😀
                      </button>
                      {reactionPickerMsgId === msg.id && (
                        <div
                          className="reaction-picker"
                          onClick={(e) => e.stopPropagation()}
                        >
                          {EMOJI_LIST.map((em) => (
                            <button
                              key={em}
                              className="btn-tiny"
                              onClick={() => handleReact(msg.id, em)}
                            >
                              {em}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>

                    {/* Message Actions */}
                    <div className="message-actions">
                      {isOwn && (
                        <button
                          className="btn-tiny"
                          onClick={() => {
                            setEditingId(msg.id);
                            setEditContent(msg.content);
                          }}
                        >
                          Edit
                        </button>
                      )}
                      {msg.editedAt && (
                        <button
                          className="btn-tiny"
                          onClick={() => handleViewHistory(msg.id)}
                        >
                          History
                        </button>
                      )}
                    </div>

                    {/* Read receipts */}
                    {seenBy.length > 0 && (
                      <div className="seen-by">
                        Seen by {seenBy.map((r) => r.username).join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            <div className="typing-indicator">
              {typingUsers.length === 1 && (
                <span>{typingUsers[0]} is typing...</span>
              )}
              {typingUsers.length === 2 && (
                <span>
                  {typingUsers[0]} and {typingUsers[1]} are typing...
                </span>
              )}
              {typingUsers.length > 2 && (
                <span>Multiple users are typing...</span>
              )}
            </div>

            {/* Message Input */}
            <div className="input-area">
              {showMsgOptions && (
                <div className="msg-options">
                  <label className="msg-option-label">
                    Ephemeral (seconds):
                    <input
                      className="input-tiny"
                      type="number"
                      min="10"
                      max="3600"
                      placeholder="e.g. 60"
                      value={ephemeralSecs}
                      onChange={(e) =>
                        setEphemeralSecs(
                          e.target.value ? Number(e.target.value) : '',
                        )
                      }
                    />
                  </label>
                  <label className="msg-option-label">
                    Schedule at:
                    <input
                      className="input-tiny"
                      type="datetime-local"
                      value={scheduleAt}
                      onChange={(e) => setScheduleAt(e.target.value)}
                    />
                  </label>
                </div>
              )}
              <form onSubmit={handleSendMessage} className="send-form">
                <button
                  type="button"
                  className="btn-sm btn-secondary"
                  onClick={() => setShowMsgOptions((v) => !v)}
                  title="Schedule / Ephemeral"
                >
                  ⚙️
                </button>
                <input
                  className="msg-input"
                  placeholder={`Message #${currentRoom.name}...`}
                  value={msgInput}
                  onChange={(e) => {
                    setMsgInput(e.target.value);
                    handleTyping();
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') {
                      setEditingId(null);
                    }
                  }}
                  maxLength={2000}
                />
                <button className="btn-sm btn-primary" type="submit">
                  {scheduleAt ? '⏰ Schedule' : 'Send'}
                </button>
              </form>
            </div>
          </>
        ) : (
          <div className="no-room">
            <span className="no-room-elephant">🐘</span>
            <h2>Welcome to PostgreSQL Chat</h2>
            <p>Select or create a room to start chatting</p>
          </div>
        )}
      </main>

      {/* ─── Members Panel ─── */}
      {showMembers && currentRoomId && (
        <aside className="members-panel">
          <div className="members-header">Members</div>
          {members
            .filter((m) => !m.isBanned)
            .map((member) => {
              const isMe = member.userId === currentUser.id;
              const isOnline = member.status === 'online';
              return (
                <div key={member.userId} className="member-item">
                  <span
                    className="status-dot"
                    style={{ background: statusColor(member.status) }}
                  />
                  <div className="member-info">
                    <span className="member-name">
                      {member.username}
                      {isMe ? ' (you)' : ''}
                      {member.isAdmin ? ' 👑' : ''}
                    </span>
                    <span className="member-status">
                      {isOnline
                        ? statusLabel(member.status)
                        : `Last active ${formatRelative(member.lastActive)}`}
                    </span>
                  </div>
                  {isAdmin && !isMe && (
                    <div className="member-actions">
                      <button
                        className="btn-tiny"
                        onClick={() => handleKick(member.userId)}
                        title="Kick"
                      >
                        🚫
                      </button>
                      <button
                        className="btn-tiny btn-danger"
                        onClick={() => handleBan(member.userId)}
                        title="Ban"
                      >
                        🔨
                      </button>
                      {!member.isAdmin && (
                        <button
                          className="btn-tiny btn-secondary"
                          onClick={() => handlePromote(member.userId)}
                          title="Promote to admin"
                        >
                          👑
                        </button>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
        </aside>
      )}

      {/* ─── Edit History Modal ─── */}
      {historyMsgId !== null && (
        <div className="modal-overlay" onClick={() => setHistoryMsgId(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              Edit History
              <button
                className="btn-tiny"
                onClick={() => setHistoryMsgId(null)}
              >
                ✕
              </button>
            </div>
            <div className="modal-body">
              {historyRecords.length === 0 ? (
                <p>No edit history.</p>
              ) : (
                historyRecords.map((rec) => (
                  <div key={rec.id} className="history-item">
                    <span className="history-time">
                      {new Date(rec.editedAt).toLocaleString()}
                    </span>
                    <span className="history-content">{rec.oldContent}</span>
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
