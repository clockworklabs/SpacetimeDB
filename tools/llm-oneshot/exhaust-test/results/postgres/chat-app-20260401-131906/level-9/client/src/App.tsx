import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

// ── Types ─────────────────────────────────────────────────────────────────────
interface User { id: number; name: string; }
interface UserStatus { userId: number; status: string; lastActiveAt: string; }

interface Room {
  id: number;
  name: string;
  createdBy: number;
  memberIds: number[];
  adminIds: number[];
  unreadCount: number;
  isPrivate: boolean;
  isDm: boolean;
}

interface Invitation {
  id: number;
  roomId: number;
  roomName: string;
  inviterId: number;
  inviterName: string;
  status: string;
  createdAt: string;
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
  parentMessageId?: number | null;
  replyCount?: number;
  replyPreview?: string | null;
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

  const [userStatuses, setUserStatuses] = useState<Map<number, UserStatus>>(new Map());
  const [myStatus, setMyStatus] = useState<string>('online');
  const autoAwayTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [scheduleMode, setScheduleMode] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [expiresAfterSeconds, setExpiresAfterSeconds] = useState<number | null>(null);
  const [now, setNow] = useState(() => Date.now());

  const [newRoomName, setNewRoomName] = useState('');
  const [isPrivateRoom, setIsPrivateRoom] = useState(false);
  const [messageInput, setMessageInput] = useState('');

  const [invitations, setInvitations] = useState<Invitation[]>([]);
  const [showInviteModal, setShowInviteModal] = useState(false);
  const [inviteUsername, setInviteUsername] = useState('');
  const [inviteError, setInviteError] = useState('');

  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editInput, setEditInput] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);

  const [showAdminPanel, setShowAdminPanel] = useState(false);
  const [kickedNotice, setKickedNotice] = useState<string | null>(null);

  const [threadParentId, setThreadParentId] = useState<number | null>(null);
  const [threadMessages, setThreadMessages] = useState<Message[]>([]);
  const [threadInput, setThreadInput] = useState('');
  const threadMessagesEndRef = useRef<HTMLDivElement>(null);

  const socketRef = useRef<Socket | null>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const isTypingRef = useRef(false);

  // ── Helpers ─────────────────────────────────────────────────────────────────
  const getUserName = useCallback((userId: number) => {
    return allUsers.find(u => u.id === userId)?.name ?? `User ${userId}`;
  }, [allUsers]);

  async function handleSetStatus(status: string) {
    if (!currentUser) return;
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
    setMyStatus(status);
  }

  function getStatusColor(status: string): string {
    switch (status) {
      case 'online': return '#22c55e'; // green
      case 'away': return '#f59e0b';   // amber
      case 'do-not-disturb': return '#ef4444'; // red
      case 'invisible': return '#6b7280'; // gray
      default: return '#6b7280';
    }
  }

  function getLastActiveText(userId: number): string {
    const s = userStatuses.get(userId);
    if (!s || !s.lastActiveAt) return '';
    const ms = Date.now() - new Date(s.lastActiveAt).getTime();
    const mins = Math.floor(ms / 60000);
    if (mins < 1) return 'Last active just now';
    if (mins < 60) return `Last active ${mins}m ago`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `Last active ${hrs}h ago`;
    return `Last active ${Math.floor(hrs / 24)}d ago`;
  }

  const currentRoom = rooms.find(r => r.id === currentRoomId) ?? null;
  const isMember = currentRoom?.memberIds.includes(currentUser?.id ?? -1) ?? false;
  const isAdmin = currentRoom?.adminIds.includes(currentUser?.id ?? -1) ?? false;

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

    socket.on('permission:kicked', ({ roomId }: { roomId: number }) => {
      setCurrentRoomId(curr => {
        if (curr === roomId) {
          setMessages([]);
          setKickedNotice(`You were kicked from this room.`);
        }
        return curr === roomId ? null : curr;
      });
      // Update room membership locally
      setRooms(prev => prev.map(r =>
        r.id === roomId
          ? { ...r, memberIds: r.memberIds.filter(id => id !== currentUser.id) }
          : r
      ));
    });

    socket.on('permission:promoted', ({ roomId, userId }: { roomId: number; userId: number }) => {
      setRooms(prev => prev.map(r =>
        r.id === roomId && !r.adminIds.includes(userId)
          ? { ...r, adminIds: [...r.adminIds, userId] }
          : r
      ));
    });

    socket.on('thread:reply', ({ parentMessageId, reply, replyCount, replyPreview }: { parentMessageId: number; reply: Message; replyCount: number; replyPreview: string }) => {
      setMessages(prev => prev.map(m =>
        m.id === parentMessageId ? { ...m, replyCount, replyPreview } : m
      ));
      setThreadParentId(curr => {
        if (curr === parentMessageId) {
          setThreadMessages(prev => prev.find(m => m.id === reply.id) ? prev : [...prev, reply]);
        }
        return curr;
      });
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

    socket.on('invitation:received', (inv: Invitation) => {
      setInvitations(prev => prev.find(i => i.id === inv.id) ? prev : [...prev, inv]);
    });

    socket.on('user:status', (status: UserStatus) => {
      setUserStatuses(prev => {
        const next = new Map(prev);
        next.set(status.userId, status);
        return next;
      });
      if (status.userId === currentUser.id) {
        setMyStatus(status.status);
      }
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
    fetch(`/api/users/${currentUser.id}/invitations`).then(r => r.json()).then(setInvitations);
    fetch('/api/users/statuses').then(r => r.json()).then((statuses: UserStatus[]) => {
      const map = new Map<number, UserStatus>();
      for (const s of statuses) map.set(s.userId, s);
      setUserStatuses(map);
    });
  }, [currentUser]);

  // ── Auto-away on inactivity ───────────────────────────────────────────────────
  useEffect(() => {
    if (!currentUser) return;
    const AUTO_AWAY_MS = 5 * 60 * 1000; // 5 minutes

    function resetTimer() {
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
      // Notify server of activity
      socketRef.current?.emit('user:activity', { userId: currentUser!.id });
      autoAwayTimerRef.current = setTimeout(() => {
        // Set away after inactivity
        fetch(`/api/users/${currentUser!.id}/status`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ status: 'away' }),
        });
      }, AUTO_AWAY_MS);
    }

    const events = ['mousemove', 'keydown', 'mousedown', 'touchstart'];
    events.forEach(e => window.addEventListener(e, resetTimer, { passive: true }));
    resetTimer(); // Start timer on mount

    return () => {
      events.forEach(e => window.removeEventListener(e, resetTimer));
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
    };
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

  // ── Load thread replies when panel opens ────────────────────────────────────
  useEffect(() => {
    if (!threadParentId) { setThreadMessages([]); return; }
    fetch(`/api/messages/${threadParentId}/thread`)
      .then(r => r.json())
      .then(setThreadMessages);
  }, [threadParentId]);

  // ── Scroll thread to bottom on new replies ──────────────────────────────────
  useEffect(() => {
    threadMessagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [threadMessages]);

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
      body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id, isPrivate: isPrivateRoom }),
    });
    if (res.ok) {
      const room: Room = await res.json();
      setNewRoomName('');
      setIsPrivateRoom(false);
      // Private rooms are only emitted to creator via socket, add directly from response
      if (room.isPrivate) {
        setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
      }
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

  async function handleKick(targetUserId: number) {
    if (!currentRoomId || !currentUser) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    if (!res.ok) {
      const err = await res.json();
      alert(err.error ?? 'Failed to kick user');
    }
  }

  async function handlePromote(targetUserId: number) {
    if (!currentRoomId || !currentUser) return;
    const res = await fetch(`/api/rooms/${currentRoomId}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    if (!res.ok) {
      const err = await res.json();
      alert(err.error ?? 'Failed to promote user');
    }
  }

  function handleSelectRoom(roomId: number) {
    setCurrentRoomId(roomId);
  }

  async function handleInviteUser(e: React.FormEvent) {
    e.preventDefault();
    if (!inviteUsername.trim() || !currentRoomId || !currentUser) return;
    setInviteError('');
    const res = await fetch(`/api/rooms/${currentRoomId}/invite`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ inviterId: currentUser.id, inviteeUsername: inviteUsername.trim() }),
    });
    if (res.ok) {
      setInviteUsername('');
      setShowInviteModal(false);
    } else {
      const err = await res.json();
      setInviteError(err.error ?? 'Failed to invite user');
    }
  }

  async function handleAcceptInvitation(invitationId: number) {
    if (!currentUser) return;
    const res = await fetch(`/api/invitations/${invitationId}/accept`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    if (res.ok) {
      const { room } = await res.json();
      setInvitations(prev => prev.filter(i => i.id !== invitationId));
      setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
      setCurrentRoomId(room.id);
    }
  }

  async function handleDeclineInvitation(invitationId: number) {
    if (!currentUser) return;
    await fetch(`/api/invitations/${invitationId}/decline`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setInvitations(prev => prev.filter(i => i.id !== invitationId));
  }

  async function handleStartDm(targetUserId: number) {
    if (!currentUser) return;
    const res = await fetch('/api/dms', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, targetUserId }),
    });
    if (res.ok) {
      const room: Room = await res.json();
      setRooms(prev => prev.find(r => r.id === room.id) ? prev : [...prev, room]);
      setCurrentRoomId(room.id);
    }
  }

  function getDmDisplayName(room: Room): string {
    if (!currentUser) return room.name;
    const otherId = room.memberIds.find(id => id !== currentUser.id);
    return `@${otherId ? getUserName(otherId) : room.name}`;
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

  // ── Threading ────────────────────────────────────────────────────────────────
  function handleOpenThread(messageId: number) {
    setThreadParentId(messageId);
    setThreadInput('');
  }

  function handleCloseThread() {
    setThreadParentId(null);
    setThreadInput('');
  }

  async function handleSendReply(e: React.FormEvent) {
    e.preventDefault();
    if (!threadInput.trim() || !threadParentId || !currentRoomId || !currentUser) return;
    const content = threadInput.trim();
    setThreadInput('');
    await fetch(`/api/rooms/${currentRoomId}/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, content, parentMessageId: threadParentId }),
    });
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
          <div className="online-dot" style={{ background: getStatusColor(myStatus) }} />
          <div style={{ flex: 1 }}>
            <div><strong>{currentUser.name}</strong></div>
            <select
              value={myStatus}
              onChange={e => handleSetStatus(e.target.value)}
              style={{ background: 'transparent', border: 'none', color: 'var(--text-muted)', fontSize: '0.8rem', cursor: 'pointer', padding: 0 }}
              title="Set your status"
            >
              <option value="online">🟢 Online</option>
              <option value="away">🟡 Away</option>
              <option value="do-not-disturb">🔴 Do Not Disturb</option>
              <option value="invisible">⚫ Invisible</option>
            </select>
          </div>
        </div>

        {invitations.length > 0 && (
          <div style={{ padding: '8px 12px', background: 'var(--bg-secondary)', borderBottom: '1px solid var(--border)' }}>
            <div style={{ fontWeight: 600, fontSize: '0.75rem', color: 'var(--text-muted)', marginBottom: 4 }}>INVITATIONS</div>
            {invitations.map(inv => (
              <div key={inv.id} style={{ fontSize: '0.8rem', marginBottom: 6 }}>
                <span style={{ color: 'var(--text)' }}>
                  <strong>{inv.inviterName}</strong> invited you to <strong>{inv.roomName}</strong>
                </span>
                <div style={{ display: 'flex', gap: 4, marginTop: 4 }}>
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={() => handleAcceptInvitation(inv.id)}
                    style={{ fontSize: '0.7rem', padding: '2px 8px' }}
                  >Accept</button>
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => handleDeclineInvitation(inv.id)}
                    style={{ fontSize: '0.7rem', padding: '2px 8px' }}
                  >Decline</button>
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="section-title">Rooms</div>
        <div className="room-list">
          {rooms.filter(r => !r.isDm).map(room => (
            <div
              key={room.id}
              className={`room-item${currentRoomId === room.id ? ' active' : ''}`}
              onClick={() => handleSelectRoom(room.id)}
            >
              <span className="room-item-name">{room.isPrivate ? '🔒' : '#'} {room.name}</span>
              {room.unreadCount > 0 && (
                <span className="unread-badge">{room.unreadCount > 99 ? '99+' : room.unreadCount}</span>
              )}
            </div>
          ))}
          {rooms.filter(r => !r.isDm).length === 0 && (
            <div style={{ padding: '12px 16px', color: 'var(--text-muted)', fontSize: '0.85rem' }}>
              No rooms yet
            </div>
          )}
        </div>

        {rooms.filter(r => r.isDm).length > 0 && (
          <>
            <div className="section-title">Direct Messages</div>
            <div className="room-list">
              {rooms.filter(r => r.isDm).map(room => (
                <div
                  key={room.id}
                  className={`room-item${currentRoomId === room.id ? ' active' : ''}`}
                  onClick={() => handleSelectRoom(room.id)}
                >
                  <span className="room-item-name">{getDmDisplayName(room)}</span>
                  {room.unreadCount > 0 && (
                    <span className="unread-badge">{room.unreadCount > 99 ? '99+' : room.unreadCount}</span>
                  )}
                </div>
              ))}
            </div>
          </>
        )}

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
          <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: '0.78rem', color: 'var(--text-muted)', marginTop: 4, cursor: 'pointer', paddingLeft: 4 }}>
            <input
              type="checkbox"
              checked={isPrivateRoom}
              onChange={e => setIsPrivateRoom(e.target.checked)}
              style={{ cursor: 'pointer' }}
            />
            Private (invite only)
          </label>
        </div>

        {/* Online users + presence */}
        <div className="online-section">
          <div className="section-title" style={{ padding: '0 0 6px' }}>Users</div>
          {allUsers.length === 0 && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.82rem' }}>Nobody online</div>
          )}
          {allUsers.map(u => {
            const isOnline = onlineUserIds.includes(u.id);
            const status = userStatuses.get(u.id);
            const displayStatus = isOnline ? (status?.status ?? 'online') : 'offline';
            const lastActive = !isOnline ? getLastActiveText(u.id) : '';
            const isMe = u.id === currentUser.id;
            return (
              <div key={u.id} className="online-user" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: 1 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6, width: '100%' }}>
                  <div className="online-dot" style={{ background: isOnline ? getStatusColor(displayStatus) : '#6b7280', flexShrink: 0 }} />
                  <span style={{ fontSize: '0.875rem', flex: 1 }}>
                    {u.name}{isMe ? ' (you)' : ''}
                  </span>
                  {!isMe && (
                    <button
                      className="btn btn-ghost btn-sm"
                      onClick={() => handleStartDm(u.id)}
                      title={`DM ${u.name}`}
                      style={{ fontSize: '0.7rem', padding: '1px 6px', opacity: 0.7 }}
                    >DM</button>
                  )}
                </div>
                {lastActive && (
                  <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)', paddingLeft: 18 }}>{lastActive}</span>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Chat area */}
      <div className="chat-area">
        {kickedNotice && !currentRoom && (
          <div style={{ background: 'var(--danger)', color: '#fff', padding: '10px 16px', textAlign: 'center', fontWeight: 500 }}>
            {kickedNotice}
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => setKickedNotice(null)}
              style={{ marginLeft: 12, color: '#fff', opacity: 0.8 }}
            >Dismiss</button>
          </div>
        )}
        {!currentRoom ? (
          <div className="empty-state">Select a room to start chatting</div>
        ) : (
          <>
            <div className="chat-header">
              <h2>{currentRoom.isDm ? getDmDisplayName(currentRoom) : (currentRoom.isPrivate ? `🔒 ${currentRoom.name}` : `# ${currentRoom.name}`)}</h2>
              <span className="member-info">{currentRoom.memberIds.length} members</span>
              {isAdmin && (
                <span style={{ fontSize: '0.75rem', color: 'var(--accent)', fontWeight: 600, marginLeft: 8 }}>ADMIN</span>
              )}
              {isAdmin && (
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setShowAdminPanel(p => !p)}
                  title="Manage members"
                  style={{ marginLeft: 8 }}
                >
                  {showAdminPanel ? '▲ Members' : '▼ Members'}
                </button>
              )}
              {isMember && currentRoom.isPrivate && !currentRoom.isDm && (
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => { setShowInviteModal(true); setInviteError(''); setInviteUsername(''); }}
                  style={{ marginLeft: 8 }}
                >+ Invite</button>
              )}
              {isMember ? (
                <button className="btn btn-ghost btn-sm btn-danger" onClick={handleLeaveRoom}>Leave</button>
              ) : (
                <button className="btn btn-primary btn-sm" onClick={handleJoinRoom}>Join</button>
              )}
            </div>

            {/* Admin panel */}
            {isAdmin && showAdminPanel && (
              <div style={{ background: 'var(--bg-secondary)', borderBottom: '1px solid var(--border)', padding: '10px 16px' }}>
                <div style={{ fontWeight: 600, fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 8 }}>MEMBERS</div>
                {currentRoom.memberIds.map(uid => {
                  const isThisAdmin = currentRoom.adminIds.includes(uid);
                  const isMe = uid === currentUser.id;
                  return (
                    <div key={uid} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', fontSize: '0.875rem' }}>
                      <span style={{ flex: 1 }}>
                        {getUserName(uid)}
                        {isMe && <span style={{ color: 'var(--text-muted)', marginLeft: 4 }}>(you)</span>}
                        {isThisAdmin && <span style={{ color: 'var(--accent)', marginLeft: 6, fontSize: '0.75rem', fontWeight: 600 }}>★ Admin</span>}
                      </span>
                      {!isMe && (
                        <>
                          <button
                            className="btn btn-ghost btn-sm btn-danger"
                            onClick={() => handleKick(uid)}
                            title={`Kick ${getUserName(uid)}`}
                            style={{ fontSize: '0.75rem', padding: '2px 8px' }}
                          >Kick</button>
                          {!isThisAdmin && (
                            <button
                              className="btn btn-ghost btn-sm"
                              onClick={() => handlePromote(uid)}
                              title={`Promote ${getUserName(uid)} to admin`}
                              style={{ fontSize: '0.75rem', padding: '2px 8px', color: 'var(--accent)' }}
                            >Promote</button>
                          )}
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            <div className="chat-main">
              <div className="messages-column">
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
                    const isThreadOpen = threadParentId === msg.id;
                    return (
                      <div key={msg.id} className={`message${isEphemeral ? ' ephemeral-message' : ''}${isThreadOpen ? ' thread-active' : ''}`}>
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
                        {/* Thread reply count / reply button */}
                        <div className="thread-info">
                          {(msg.replyCount ?? 0) > 0 ? (
                            <button className="thread-reply-count-btn" onClick={() => handleOpenThread(msg.id)}>
                              💬 {msg.replyCount} {msg.replyCount === 1 ? 'reply' : 'replies'}
                              {msg.replyPreview && (
                                <span className="thread-preview-text">
                                  {' — '}{msg.replyPreview.slice(0, 50)}{(msg.replyPreview?.length ?? 0) > 50 ? '...' : ''}
                                </span>
                              )}
                            </button>
                          ) : (
                            <button className="thread-btn" onClick={() => handleOpenThread(msg.id)} title="Reply in thread">
                              💬 Reply
                            </button>
                          )}
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
              </div>

              {/* Thread panel */}
              {threadParentId !== null && (() => {
                const parentMsg = messages.find(m => m.id === threadParentId);
                return (
                  <div className="thread-panel">
                    <div className="thread-panel-header">
                      <h3>Thread</h3>
                      <button className="btn btn-ghost btn-sm" onClick={handleCloseThread}>✕</button>
                    </div>
                    {parentMsg && (
                      <div className="thread-parent-msg">
                        <div className="message-header">
                          <span className={`message-author${parentMsg.userId === currentUser.id ? ' is-me' : ''}`}>
                            {getUserName(parentMsg.userId)}
                          </span>
                          <span className="message-time">{formatTime(parentMsg.createdAt)}</span>
                        </div>
                        <div className="message-content" style={{ fontSize: '0.85rem' }}>{parentMsg.content}</div>
                      </div>
                    )}
                    <div className="thread-divider">
                      {threadMessages.length} {threadMessages.length === 1 ? 'reply' : 'replies'}
                    </div>
                    <div className="thread-replies">
                      {threadMessages.map(reply => (
                        <div key={reply.id} className="message">
                          <div className="message-header">
                            <span className={`message-author${reply.userId === currentUser.id ? ' is-me' : ''}`}>
                              {getUserName(reply.userId)}
                            </span>
                            <span className="message-time">{formatTime(reply.createdAt)}</span>
                          </div>
                          <div className="message-content">{reply.content}</div>
                        </div>
                      ))}
                      <div ref={threadMessagesEndRef} />
                    </div>
                    {isMember && (
                      <form className="thread-input-row" onSubmit={handleSendReply}>
                        <input
                          type="text"
                          placeholder="Reply in thread..."
                          value={threadInput}
                          onChange={e => setThreadInput(e.target.value)}
                          maxLength={2000}
                        />
                        <button type="submit" disabled={!threadInput.trim()}>Reply</button>
                      </form>
                    )}
                  </div>
                );
              })()}
            </div>
          </>
        )}
      </div>
      {/* Invite user modal */}
      {showInviteModal && (
        <div className="modal-overlay" onClick={() => setShowInviteModal(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <h3>Invite to {currentRoom?.name}</h3>
              <button className="btn btn-ghost btn-sm" onClick={() => setShowInviteModal(false)}>✕</button>
            </div>
            <div className="modal-body">
              <form onSubmit={handleInviteUser} style={{ display: 'flex', gap: 8 }}>
                <input
                  type="text"
                  placeholder="Username"
                  value={inviteUsername}
                  onChange={e => setInviteUsername(e.target.value)}
                  autoFocus
                  maxLength={32}
                  style={{ flex: 1, padding: '6px 10px', background: 'var(--bg-tertiary)', border: '1px solid var(--border)', borderRadius: 4, color: 'var(--text)' }}
                />
                <button type="submit" className="btn btn-primary btn-sm" disabled={!inviteUsername.trim()}>Invite</button>
              </form>
              {inviteError && <p style={{ color: 'var(--danger)', fontSize: '0.85rem', marginTop: 8 }}>{inviteError}</p>}
            </div>
          </div>
        </div>
      )}

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
