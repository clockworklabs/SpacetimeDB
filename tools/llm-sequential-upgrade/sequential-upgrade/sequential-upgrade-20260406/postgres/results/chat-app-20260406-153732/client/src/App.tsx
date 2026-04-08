import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

type UserStatus = 'online' | 'away' | 'dnd' | 'invisible' | 'offline';

interface User {
  id: number;
  name: string;
  online: boolean;
  status?: UserStatus;
  lastSeen?: string;
}

interface Room {
  id: number;
  name: string;
  isPrivate: boolean;
  isDm: boolean;
  dmPartnerName?: string | null;
  unreadCount: number;
  joined: boolean;
  activityLevel?: 'hot' | 'active' | null;
}

interface Invitation {
  id: number;
  roomId: number;
  roomName: string;
  inviterId: number;
  inviterName: string;
  createdAt: string;
}

interface ReadBy {
  userId: number;
  userName: string;
}

interface Reaction {
  emoji: string;
  userId: number;
  userName: string;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  userName: string;
  content: string;
  expiresAt?: string | null;
  editedAt?: string | null;
  parentMessageId?: number | null;
  createdAt: string;
  readBy: ReadBy[];
  reactions: Reaction[];
  replyCount?: number;
}

interface RoomMember {
  userId: number;
  name: string;
  isAdmin: boolean;
}

interface EditHistoryEntry {
  id: number;
  messageId: number;
  content: string;
  editedAt: string;
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  userId: number;
  content: string;
  scheduledFor: string;
  createdAt: string;
  roomName: string;
}

export default function App() {
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [nameInput, setNameInput] = useState('');
  const [nameError, setNameError] = useState('');
  const [nameLoading, setNameLoading] = useState(false);

  const [rooms, setRooms] = useState<Room[]>([]);
  const [activeRoomId, setActiveRoomId] = useState<number | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [typingUsers, setTypingUsers] = useState<Map<number, string>>(new Map());
  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [connected, setConnected] = useState(false);
  const [messagesLoading, setMessagesLoading] = useState(false);

  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomPrivate, setNewRoomPrivate] = useState(false);
  const [roomError, setRoomError] = useState('');

  const [invitations, setInvitations] = useState<Invitation[]>([]);
  const [showInvitationsPanel, setShowInvitationsPanel] = useState(false);

  const [showInviteUserModal, setShowInviteUserModal] = useState(false);
  const [inviteUsername, setInviteUsername] = useState('');
  const [inviteError, setInviteError] = useState('');
  const [inviteSuccess, setInviteSuccess] = useState('');

  const [messageInput, setMessageInput] = useState('');
  const [isScrolledUp, setIsScrolledUp] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState<number>(0); // 0 = not ephemeral
  const [now, setNow] = useState(Date.now());

  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleContent, setScheduleContent] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [scheduleError, setScheduleError] = useState('');
  const [showScheduledPanel, setShowScheduledPanel] = useState(false);

  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [showMembersPanel, setShowMembersPanel] = useState(false);
  const [kickedNotice, setKickedNotice] = useState<string | null>(null);

  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editInput, setEditInput] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<EditHistoryEntry[]>([]);

  // Drafts: roomId -> content
  const [drafts, setDrafts] = useState<Map<number, string>>(new Map());
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Threading
  const [activeThreadParentId, setActiveThreadParentId] = useState<number | null>(null);
  const [threadParentMsg, setThreadParentMsg] = useState<Message | null>(null);
  const [threadMessages, setThreadMessages] = useState<Message[]>([]);
  const [threadInput, setThreadInput] = useState('');
  const [threadLoading, setThreadLoading] = useState(false);
  const activeThreadParentIdRef = useRef<number | null>(null);
  useEffect(() => { activeThreadParentIdRef.current = activeThreadParentId; }, [activeThreadParentId]);

  // Rich presence
  const [myStatus, setMyStatus] = useState<UserStatus>('online');
  const [allUsers, setAllUsers] = useState<User[]>([]);
  // Whether the current auto-away was set automatically (not manually)
  const autoAwayRef = useRef(false);
  const inactivityTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const socketRef = useRef<Socket | null>(null);
  const activeRoomIdRef = useRef<number | null>(null);
  const currentUserRef = useRef<User | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Keep refs in sync
  useEffect(() => { activeRoomIdRef.current = activeRoomId; }, [activeRoomId]);
  useEffect(() => { currentUserRef.current = currentUser; }, [currentUser]);

  // Tick every second for ephemeral countdown
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  // Auto-away after 5 minutes of inactivity
  useEffect(() => {
    if (!currentUser || !socketRef.current) return;

    const INACTIVITY_MS = 5 * 60 * 1000;

    function resetInactivityTimer() {
      if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
      // If we went away automatically, come back online on activity
      if (autoAwayRef.current) {
        autoAwayRef.current = false;
        setMyStatus('online');
        socketRef.current?.emit('set_status', { status: 'online' });
      }
      inactivityTimerRef.current = setTimeout(() => {
        // Only auto-away if currently online
        setMyStatus((prev) => {
          if (prev === 'online') {
            autoAwayRef.current = true;
            socketRef.current?.emit('set_status', { status: 'away' });
            return 'away';
          }
          return prev;
        });
      }, INACTIVITY_MS);
    }

    const events = ['mousemove', 'keydown', 'click', 'scroll', 'touchstart'];
    events.forEach((e) => window.addEventListener(e, resetInactivityTimer, { passive: true }));
    resetInactivityTimer();

    return () => {
      events.forEach((e) => window.removeEventListener(e, resetInactivityTimer));
      if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
    };
  }, [currentUser]); // eslint-disable-line react-hooks/exhaustive-deps

  const scrollToBottom = useCallback((smooth = true) => {
    messagesEndRef.current?.scrollIntoView({ behavior: smooth ? 'smooth' : 'instant' });
  }, []);

  // Auto-scroll when messages arrive
  useEffect(() => {
    if (!isScrolledUp) scrollToBottom();
  }, [messages, isScrolledUp, scrollToBottom]);

  // ── Socket setup ────────────────────────────────────────────────────────────

  useEffect(() => {
    const socket = io();
    socketRef.current = socket;

    socket.on('connect', () => {
      setConnected(true);
      // Re-register and re-join on reconnect (socket.io auto-reconnects with a new socket ID)
      const user = currentUserRef.current;
      const roomId = activeRoomIdRef.current;
      if (user) socket.emit('register', { userId: user.id, userName: user.name });
      if (roomId) socket.emit('join_room', { roomId });
    });
    socket.on('disconnect', () => setConnected(false));

    socket.on('message', (msg: Message) => {
      const roomId = activeRoomIdRef.current;
      const user = currentUserRef.current;

      if (msg.roomId === roomId) {
        setMessages((prev) => [...prev, msg]);
        // Auto-mark as read since user is viewing this room
        if (user) socket.emit('mark_read', { messageId: msg.id });
      } else {
        setRooms((prev) =>
          prev.map((r) => r.id === msg.roomId ? { ...r, unreadCount: r.unreadCount + 1 } : r)
        );
      }
    });

    socket.on('typing', ({ userId, userName, typing }: { userId: number; userName: string; typing: boolean }) => {
      setTypingUsers((prev) => {
        const next = new Map(prev);
        if (typing) next.set(userId, userName);
        else next.delete(userId);
        return next;
      });
    });

    socket.on('read_receipt', ({ messageId, userId, userName }: { messageId: number; userId: number; userName: string }) => {
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id === messageId && m.userId !== userId && !m.readBy.some((r) => r.userId === userId)) {
            return { ...m, readBy: [...m.readBy, { userId, userName }] };
          }
          return m;
        })
      );
    });

    socket.on('bulk_read', ({ messageIds, userId, userName }: { messageIds: number[]; userId: number; userName: string }) => {
      const idSet = new Set(messageIds);
      setMessages((prev) =>
        prev.map((m) => {
          if (idSet.has(m.id) && m.userId !== userId && !m.readBy.some((r) => r.userId === userId)) {
            return { ...m, readBy: [...m.readBy, { userId, userName }] };
          }
          return m;
        })
      );
    });

    socket.on('user_status', ({ userId, online, name, status, lastSeen }: { userId: number; online: boolean; name: string; status?: UserStatus; lastSeen?: string }) => {
      const effectiveStatus: UserStatus = status ?? (online ? 'online' : 'offline');
      setOnlineUsers((prev) => {
        if (online) {
          if (prev.some((u) => u.id === userId)) {
            return prev.map((u) => u.id === userId ? { ...u, online: true, status: effectiveStatus } : u);
          }
          return [...prev, { id: userId, name, online: true, status: effectiveStatus }];
        } else {
          return prev.filter((u) => u.id !== userId);
        }
      });
      setAllUsers((prev) =>
        prev.map((u) =>
          u.id === userId
            ? { ...u, online, status: effectiveStatus, lastSeen: lastSeen ?? u.lastSeen }
            : u
        )
      );
    });

    socket.on('room_created', (room: Room) => {
      setRooms((prev) => {
        if (prev.some((r) => r.id === room.id)) return prev;
        return [...prev, { ...room, unreadCount: 0, joined: false }];
      });
    });

    socket.on('scheduled_message_sent', ({ id }: { id: number }) => {
      setScheduledMessages((prev) => prev.filter((s) => s.id !== id));
    });

    socket.on('message_expired', ({ messageId }: { messageId: number }) => {
      setMessages((prev) => prev.filter((m) => m.id !== messageId));
    });

    socket.on('reaction_updated', ({ messageId, userId, userName, emoji, added }: { messageId: number; userId: number; userName: string; emoji: string; added: boolean }) => {
      setMessages((prev) =>
        prev.map((m) => {
          if (m.id !== messageId) return m;
          if (added) {
            // Add reaction if not already present
            if (m.reactions.some((r) => r.userId === userId && r.emoji === emoji)) return m;
            return { ...m, reactions: [...m.reactions, { emoji, userId, userName }] };
          } else {
            // Remove reaction
            return { ...m, reactions: m.reactions.filter((r) => !(r.userId === userId && r.emoji === emoji)) };
          }
        })
      );
    });

    socket.on('message_edited', ({ messageId, content, editedAt }: { messageId: number; content: string; editedAt: string }) => {
      setMessages((prev) =>
        prev.map((m) => m.id === messageId ? { ...m, content, editedAt } : m)
      );
    });

    socket.on('kicked_from_room', ({ roomId }: { roomId: number }) => {
      // Direct notification to the kicked user — always redirect away
      setKickedNotice('You have been kicked from this room.');
      setActiveRoomId(null);
      setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: false } : r));
    });

    socket.on('user_kicked', ({ userId, roomId }: { userId: number; roomId: number }) => {
      const user = currentUserRef.current;
      if (user && user.id === userId) {
        // Fallback for kicked user in case direct emit was missed
        const wasActive = activeRoomIdRef.current === roomId;
        setActiveRoomId((prev) => (prev === roomId ? null : prev));
        if (wasActive) setKickedNotice('You have been kicked from this room.');
        setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: false } : r));
      }
      setRoomMembers((prev) => prev.filter((m) => m.userId !== userId));
    });

    socket.on('user_promoted', ({ userId }: { userId: number }) => {
      setRoomMembers((prev) =>
        prev.map((m) => m.userId === userId ? { ...m, isAdmin: true } : m)
      );
    });

    socket.on('member_joined', ({ userId, name, isAdmin }: { userId: number; name: string; isAdmin: boolean }) => {
      setRoomMembers((prev) => {
        if (prev.some((m) => m.userId === userId)) return prev;
        return [...prev, { userId, name, isAdmin }];
      });
    });

    socket.on('member_left', ({ userId }: { userId: number }) => {
      setRoomMembers((prev) => prev.filter((m) => m.userId !== userId));
    });

    socket.on('thread_reply', (msg: Message) => {
      if (activeThreadParentIdRef.current === msg.parentMessageId) {
        setThreadMessages((prev) => {
          if (prev.some((m) => m.id === msg.id)) return prev;
          return [...prev, msg];
        });
      }
    });

    socket.on('reply_count_updated', ({ messageId, replyCount }: { messageId: number; replyCount: number }) => {
      setMessages((prev) =>
        prev.map((m) => m.id === messageId ? { ...m, replyCount } : m)
      );
    });

    socket.on('invitation_received', (inv: Invitation) => {
      setInvitations((prev) => {
        if (prev.some((i) => i.id === inv.id)) return prev;
        return [inv, ...prev];
      });
    });

    socket.on('room_activity_update', ({ roomId, level }: { roomId: number; level: 'hot' | 'active' | null }) => {
      setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, activityLevel: level } : r));
    });

    socket.on('draft_updated', ({ roomId, content }: { roomId: number; content: string }) => {
      setDrafts((prev) => {
        const next = new Map(prev);
        if (content) next.set(roomId, content);
        else next.delete(roomId);
        return next;
      });
      // If currently viewing that room, update the input
      if (activeRoomIdRef.current === roomId) {
        setMessageInput(content);
      }
    });

    return () => { socket.disconnect(); };
  }, []);

  // ── Register with server when user is set ───────────────────────────────────

  useEffect(() => {
    if (!currentUser || !socketRef.current) return;
    const socket = socketRef.current;

    socket.emit('register', { userId: currentUser.id, userName: currentUser.name });

    // Fetch rooms and online users
    Promise.all([
      fetch(`/api/rooms?userId=${currentUser.id}`).then((r) => r.json()),
      fetch('/api/rooms/activity').then((r) => r.json()),
    ])
      .then(([roomsData, activityData]: [unknown, Record<string, 'hot' | 'active'>]) => {
        if (!Array.isArray(roomsData)) return;
        setRooms(roomsData.map((room: Room) => ({
          ...room,
          activityLevel: activityData[room.id] ?? null,
        })));
      })
      .catch(console.error);

    fetch('/api/users/online')
      .then((r) => r.json())
      .then(setOnlineUsers)
      .catch(console.error);

    fetch(`/api/scheduled-messages?userId=${currentUser.id}`)
      .then((r) => r.json())
      .then(setScheduledMessages)
      .catch(console.error);

    fetch('/api/users')
      .then((r) => r.json())
      .then((data: unknown) => setAllUsers(Array.isArray(data) ? data : []))
      .catch(console.error);

    fetch(`/api/invitations?userId=${currentUser.id}`)
      .then((r) => r.json())
      .then((data: unknown) => setInvitations(Array.isArray(data) ? data : []))
      .catch(console.error);

    fetch(`/api/drafts?userId=${currentUser.id}`)
      .then((r) => r.json())
      .then((data: unknown) => {
        if (!Array.isArray(data)) return;
        const map = new Map<number, string>();
        for (const d of data as { roomId: number; content: string }[]) {
          if (d.content) map.set(d.roomId, d.content);
        }
        setDrafts(map);
      })
      .catch(console.error);
  }, [currentUser]);

  // ── Join/leave socket room when active room changes ──────────────────────────

  useEffect(() => {
    const socket = socketRef.current;
    const user = currentUser;
    if (!activeRoomId || !user || !socket) return;

    socket.emit('join_room', { roomId: activeRoomId });
    setMessages([]);
    setTypingUsers(new Map());
    setRoomMembers([]);
    setShowMembersPanel(false);
    setMessagesLoading(true);
    // Restore draft for this room
    setDrafts((prev) => {
      setMessageInput(prev.get(activeRoomId) ?? '');
      return prev;
    });

    const roomIdSnapshot = activeRoomId;
    fetch(`/api/rooms/${activeRoomId}/messages?userId=${user.id}`)
      .then((r) => {
        if (r.status === 403) {
          setActiveRoomId(null);
          setRooms((prev) => prev.map((rm) => rm.id === roomIdSnapshot ? { ...rm, joined: false } : rm));
          setKickedNotice('You are banned from this room.');
          return null;
        }
        return r.json();
      })
      .then((msgs: unknown) => {
        if (msgs === null) return;
        setMessages(Array.isArray(msgs) ? msgs : []);
        setRooms((prev) => prev.map((r) => r.id === roomIdSnapshot ? { ...r, unreadCount: 0 } : r));
        scrollToBottom(false);
      })
      .catch(console.error)
      .finally(() => setMessagesLoading(false));

    return () => {
      socket.emit('leave_room', { roomId: activeRoomId });
      if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
      socket.emit('typing_stop', { roomId: activeRoomId });
    };
  }, [activeRoomId, currentUser]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Handlers ────────────────────────────────────────────────────────────────

  async function handleSetName(e: React.FormEvent) {
    e.preventDefault();
    const name = nameInput.trim();
    if (!name) return setNameError('Please enter a name');
    if (name.length > 30) return setNameError('Name must be 30 characters or fewer');

    setNameLoading(true);
    setNameError('');
    try {
      const res = await fetch('/api/users', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      });
      const data = await res.json();
      if (!res.ok) return setNameError(data.error ?? 'Failed to set name');
      setCurrentUser(data);
    } catch {
      setNameError('Network error');
    } finally {
      setNameLoading(false);
    }
  }

  async function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    const name = newRoomName.trim();
    if (!name) return setRoomError('Room name required');
    if (!currentUser) return;

    setRoomError('');
    try {
      const res = await fetch('/api/rooms', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, userId: currentUser.id, isPrivate: newRoomPrivate }),
      });
      const data = await res.json();
      if (!res.ok) return setRoomError(data.error ?? 'Failed to create room');
      setRooms((prev) => {
        if (prev.some((r) => r.id === data.id)) return prev;
        return [...prev, data];
      });
      setNewRoomName('');
      setNewRoomPrivate(false);
      setShowCreateRoom(false);
      setActiveRoomId(data.id);
    } catch {
      setRoomError('Network error');
    }
  }

  async function handleJoinRoom(roomId: number) {
    if (!currentUser) return;
    const res = await fetch(`/api/rooms/${roomId}/join`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    if (!res.ok) {
      if (res.status === 403) {
        const data = await res.json().catch(() => ({}));
        setKickedNotice((data as { error?: string }).error ?? 'You are banned from this room.');
      }
      return;
    }
    setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: true } : r));
    setActiveRoomId(roomId);
  }

  async function handleLeaveRoom(roomId: number) {
    if (!currentUser) return;
    await fetch(`/api/rooms/${roomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setRooms((prev) => prev.map((r) => r.id === roomId ? { ...r, joined: false } : r));
    if (activeRoomId === roomId) setActiveRoomId(null);
  }

  function handleSelectRoom(room: Room) {
    setKickedNotice(null);
    if (!room.joined && !room.isPrivate) {
      handleJoinRoom(room.id);
    } else {
      setActiveRoomId(room.id);
    }
  }

  async function handleScheduleMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!activeRoomId || !currentUser) return;
    const content = scheduleContent.trim();
    if (!content) return setScheduleError('Message content required');
    if (!scheduleTime) return setScheduleError('Schedule time required');

    const scheduledFor = new Date(scheduleTime);
    if (scheduledFor <= new Date()) return setScheduleError('Must be a future time');

    setScheduleError('');
    try {
      const res = await fetch('/api/scheduled-messages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ roomId: activeRoomId, userId: currentUser.id, content, scheduledFor: scheduledFor.toISOString() }),
      });
      const data = await res.json();
      if (!res.ok) return setScheduleError(data.error ?? 'Failed to schedule message');

      const activeRoom = rooms.find((r) => r.id === activeRoomId);
      setScheduledMessages((prev) => [...prev, { ...data, roomName: activeRoom?.name ?? '' }]);
      setScheduleContent('');
      setScheduleTime('');
      setShowScheduleModal(false);
    } catch {
      setScheduleError('Network error');
    }
  }

  async function handleCancelScheduled(id: number) {
    if (!currentUser) return;
    try {
      await fetch(`/api/scheduled-messages/${id}`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      setScheduledMessages((prev) => prev.filter((s) => s.id !== id));
    } catch {
      console.error('Failed to cancel scheduled message');
    }
  }

  async function handleToggleReaction(messageId: number, emoji: string) {
    if (!currentUser) return;
    try {
      await fetch(`/api/messages/${messageId}/reactions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, emoji }),
      });
    } catch {
      console.error('Failed to toggle reaction');
    }
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    const content = messageInput.trim();
    if (!content || !activeRoomId || !socketRef.current) return;

    socketRef.current.emit('send_message', {
      roomId: activeRoomId,
      content,
      ...(ephemeralDuration > 0 ? { expiresInMs: ephemeralDuration } : {}),
    });
    setMessageInput('');

    // Clear draft on send
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    socketRef.current.emit('save_draft', { roomId: activeRoomId, content: '' });
    setDrafts((prev) => { const next = new Map(prev); next.delete(activeRoomId); return next; });

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    socketRef.current.emit('typing_stop', { roomId: activeRoomId });
  }

  function handleMessageInput(e: React.ChangeEvent<HTMLInputElement>) {
    const value = e.target.value;
    setMessageInput(value);
    if (!activeRoomId || !socketRef.current) return;

    socketRef.current.emit('typing_start', { roomId: activeRoomId });

    if (typingTimeoutRef.current) clearTimeout(typingTimeoutRef.current);
    typingTimeoutRef.current = setTimeout(() => {
      socketRef.current?.emit('typing_stop', { roomId: activeRoomId });
    }, 2000);

    // Save draft (debounced)
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    draftSaveTimerRef.current = setTimeout(() => {
      socketRef.current?.emit('save_draft', { roomId: activeRoomId, content: value });
      setDrafts((prev) => {
        const next = new Map(prev);
        if (value) next.set(activeRoomId, value);
        else next.delete(activeRoomId);
        return next;
      });
    }, 500);
  }

  function handleStartEdit(msg: Message) {
    setEditingMessageId(msg.id);
    setEditInput(msg.content);
  }

  async function handleSaveEdit(e: React.FormEvent, messageId: number) {
    e.preventDefault();
    const content = editInput.trim();
    if (!content || !currentUser) return;

    try {
      await fetch(`/api/messages/${messageId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content }),
      });
      setEditingMessageId(null);
      setEditInput('');
    } catch {
      console.error('Failed to edit message');
    }
  }

  async function handleOpenMembersPanel() {
    if (!activeRoomId) return;
    try {
      const res = await fetch(`/api/rooms/${activeRoomId}/members`);
      const data = await res.json();
      setRoomMembers(Array.isArray(data) ? data : []);
    } catch {
      console.error('Failed to fetch members');
    }
    setShowMembersPanel(true);
  }

  async function handleKickUser(targetUserId: number) {
    if (!currentUser || !activeRoomId) return;
    try {
      await fetch(`/api/rooms/${activeRoomId}/kick`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
      });
    } catch {
      console.error('Failed to kick user');
    }
  }

  async function handlePromoteUser(targetUserId: number) {
    if (!currentUser || !activeRoomId) return;
    try {
      await fetch(`/api/rooms/${activeRoomId}/promote`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
      });
    } catch {
      console.error('Failed to promote user');
    }
  }

  async function handleShowHistory(messageId: number) {
    try {
      const res = await fetch(`/api/messages/${messageId}/history`);
      const data = await res.json();
      setEditHistory(Array.isArray(data) ? data : []);
      setHistoryMessageId(messageId);
    } catch {
      console.error('Failed to get edit history');
    }
  }

  async function handleOpenThread(messageId: number) {
    if (!currentUser) return;
    // Leave previous thread room
    if (activeThreadParentId !== null && socketRef.current) {
      socketRef.current.emit('leave_thread', { parentMessageId: activeThreadParentId });
    }
    setActiveThreadParentId(messageId);
    setThreadMessages([]);
    setThreadInput('');
    setThreadLoading(true);
    try {
      const res = await fetch(`/api/messages/${messageId}/thread?userId=${currentUser.id}`);
      if (!res.ok) return;
      const data = await res.json() as { parent: Message; replies: Message[] };
      setThreadParentMsg(data.parent);
      setThreadMessages(data.replies);
    } catch {
      console.error('Failed to load thread');
    } finally {
      setThreadLoading(false);
    }
    if (socketRef.current) {
      socketRef.current.emit('join_thread', { parentMessageId: messageId });
    }
  }

  function handleCloseThread() {
    if (activeThreadParentId !== null && socketRef.current) {
      socketRef.current.emit('leave_thread', { parentMessageId: activeThreadParentId });
    }
    setActiveThreadParentId(null);
    setThreadParentMsg(null);
    setThreadMessages([]);
    setThreadInput('');
  }

  function handleSendThreadReply(e: React.FormEvent) {
    e.preventDefault();
    const content = threadInput.trim();
    if (!content || !activeThreadParentId || !socketRef.current || !threadParentMsg) return;
    socketRef.current.emit('send_message', {
      roomId: threadParentMsg.roomId,
      content,
      parentMessageId: activeThreadParentId,
    });
    setThreadInput('');
  }

  function handleScroll() {
    const el = messagesContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    setIsScrolledUp(!atBottom);
  }

  function handleSetStatus(status: UserStatus) {
    if (!socketRef.current) return;
    autoAwayRef.current = false; // manual change — don't auto-restore
    setMyStatus(status);
    socketRef.current.emit('set_status', { status });
  }

  async function handleAcceptInvitation(invitationId: number) {
    if (!currentUser) return;
    try {
      const res = await fetch(`/api/invitations/${invitationId}/accept`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      if (!res.ok) return;
      const data = await res.json();
      setInvitations((prev) => prev.filter((i) => i.id !== invitationId));
      // Add room to list if returned
      if (data.room) {
        setRooms((prev) => {
          if (prev.some((r) => r.id === data.room.id)) return prev;
          return [...prev, data.room];
        });
        setActiveRoomId(data.room.id);
      }
    } catch {
      console.error('Failed to accept invitation');
    }
  }

  async function handleDeclineInvitation(invitationId: number) {
    if (!currentUser) return;
    try {
      await fetch(`/api/invitations/${invitationId}/decline`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      setInvitations((prev) => prev.filter((i) => i.id !== invitationId));
    } catch {
      console.error('Failed to decline invitation');
    }
  }

  async function handleInviteUser(e: React.FormEvent) {
    e.preventDefault();
    if (!currentUser || !activeRoomId) return;
    const name = inviteUsername.trim();
    if (!name) return setInviteError('Username required');

    setInviteError('');
    setInviteSuccess('');
    try {
      const res = await fetch(`/api/rooms/${activeRoomId}/invite`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ adminId: currentUser.id, inviteeName: name }),
      });
      const data = await res.json();
      if (!res.ok) return setInviteError(data.error ?? 'Failed to invite user');
      setInviteSuccess(`Invited ${name} successfully`);
      setInviteUsername('');
    } catch {
      setInviteError('Network error');
    }
  }

  async function handleOpenDM(partnerId: number) {
    if (!currentUser) return;
    try {
      const res = await fetch('/api/dm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, partnerId }),
      });
      if (!res.ok) return;
      const room = await res.json();
      setRooms((prev) => {
        if (prev.some((r) => r.id === room.id)) return prev;
        return [...prev, room];
      });
      setActiveRoomId(room.id);
    } catch {
      console.error('Failed to open DM');
    }
  }

  function formatLastSeen(lastSeen?: string): string {
    if (!lastSeen) return 'a while ago';
    const diff = Date.now() - new Date(lastSeen).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  const statusColor = (status?: UserStatus) => {
    switch (status) {
      case 'online': return 'var(--success)';
      case 'away': return 'var(--warning)';
      case 'dnd': return 'var(--danger)';
      default: return 'var(--text-muted)';
    }
  };

  const statusLabel = (status?: UserStatus) => {
    switch (status) {
      case 'online': return 'Online';
      case 'away': return 'Away';
      case 'dnd': return 'Do Not Disturb';
      case 'invisible': return 'Invisible';
      default: return 'Offline';
    }
  };

  // ── Render ──────────────────────────────────────────────────────────────────

  // Name modal
  if (!currentUser) {
    return (
      <div className="modal-overlay">
        <div className="modal">
          <h2>Welcome to PostgreSQL Chat</h2>
          <p>Enter a display name to get started.</p>
          <form onSubmit={handleSetName} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div className="form-field">
              <label>Display Name</label>
              <input
                className="text-input"
                type="text"
                placeholder="e.g. Alice"
                value={nameInput}
                onChange={(e) => setNameInput(e.target.value)}
                maxLength={30}
                autoFocus
              />
              {nameError && <span className="error-msg">{nameError}</span>}
            </div>
            <button className="btn btn-primary" type="submit" disabled={nameLoading || !nameInput.trim()}>
              {nameLoading ? 'Joining…' : 'Enter Chat'}
            </button>
          </form>
        </div>
      </div>
    );
  }

  const activeRoom = rooms.find((r) => r.id === activeRoomId) ?? null;
  const currentUserIsAdmin = roomMembers.find((m) => m.userId === currentUser.id)?.isAdmin ?? false;
  const typingList = Array.from(typingUsers.values()).filter((n) => n !== currentUser.name);

  let typingText = '';
  if (typingList.length === 1) typingText = `${typingList[0]} is typing`;
  else if (typingList.length === 2) typingText = `${typingList[0]} and ${typingList[1]} are typing`;
  else if (typingList.length > 2) typingText = 'Multiple users are typing';

  // Group consecutive messages from same sender (within 5 min)
  const groupedMessages: { msgs: Message[]; grouped: boolean }[] = [];
  for (const msg of messages) {
    const last = groupedMessages[groupedMessages.length - 1];
    const prevMsg = last?.msgs[last.msgs.length - 1];
    if (
      last &&
      prevMsg &&
      prevMsg.userId === msg.userId &&
      new Date(msg.createdAt).getTime() - new Date(prevMsg.createdAt).getTime() < 5 * 60 * 1000
    ) {
      last.msgs.push(msg);
    } else {
      groupedMessages.push({ msgs: [msg], grouped: false });
    }
  }

  const formatTime = (iso: string) => {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatCountdown = (expiresAt: string) => {
    const remaining = Math.max(0, new Date(expiresAt).getTime() - now);
    const secs = Math.floor(remaining / 1000);
    if (secs <= 0) return 'expiring…';
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    const s = secs % 60;
    return `${mins}m ${s}s`;
  };

  return (
    <div className="app">
      {/* ── Sidebar ── */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1>PostgreSQL Chat</h1>
          <p>Real-time powered by Postgres</p>
        </div>

        <div className="sidebar-scrollable">
          <div className="sidebar-section">
            <div className="sidebar-section-title" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span>Rooms</span>
              {invitations.length > 0 && (
                <button
                  className="btn btn-ghost btn-sm"
                  style={{ fontSize: 11, padding: '1px 6px', color: 'var(--warning)' }}
                  onClick={() => setShowInvitationsPanel((p) => !p)}
                  title="Pending invitations"
                >
                  🔔 {invitations.length}
                </button>
              )}
            </div>
            {rooms.filter(r => !r.isDm).length === 0 && (
              <div style={{ padding: '8px 16px', fontSize: 13, color: 'var(--text-muted)' }}>
                No rooms yet
              </div>
            )}
            {rooms.filter(r => !r.isDm).map((room) => (
              <div
                key={room.id}
                className={`room-item ${activeRoomId === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room)}
              >
                <div className="room-item-name">
                  <span style={{ color: 'var(--text-muted)', fontSize: 13 }}>{room.isPrivate ? '🔒' : '#'}</span>
                  <span>{room.name}</span>
                  {!room.joined && (
                    <span style={{ fontSize: 10, color: 'var(--text-muted)', marginLeft: 2 }}>+join</span>
                  )}
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                  {room.activityLevel === 'hot' && (
                    <span className="activity-badge activity-hot">🔥 Hot</span>
                  )}
                  {room.activityLevel === 'active' && (
                    <span className="activity-badge activity-active">● Active</span>
                  )}
                  {drafts.has(room.id) && activeRoomId !== room.id && (
                    <span title="Draft saved" style={{ fontSize: 11, color: 'var(--text-muted)' }}>✏️</span>
                  )}
                  {room.unreadCount > 0 && (
                    <span className="unread-badge">{room.unreadCount}</span>
                  )}
                </div>
              </div>
            ))}
          </div>

          {showCreateRoom ? (
            <form className="create-room-form" onSubmit={handleCreateRoom}>
              <input
                type="text"
                placeholder="Room name"
                value={newRoomName}
                onChange={(e) => setNewRoomName(e.target.value)}
                maxLength={50}
                autoFocus
                onKeyDown={(e) => { if (e.key === 'Escape') { setShowCreateRoom(false); setRoomError(''); setNewRoomPrivate(false); } }}
              />
              <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, color: 'var(--text-muted)', cursor: 'pointer', padding: '2px 0' }}>
                <input
                  type="checkbox"
                  checked={newRoomPrivate}
                  onChange={(e) => setNewRoomPrivate(e.target.checked)}
                />
                Private (invite-only)
              </label>
              {roomError && <span className="error-msg">{roomError}</span>}
              <div className="create-room-form-actions">
                <button className="btn btn-primary btn-sm" type="submit">Create</button>
                <button className="btn btn-ghost btn-sm" type="button" onClick={() => { setShowCreateRoom(false); setRoomError(''); setNewRoomPrivate(false); }}>Cancel</button>
              </div>
            </form>
          ) : (
            <div style={{ padding: '4px 16px 8px' }}>
              <button className="btn btn-ghost btn-sm" onClick={() => setShowCreateRoom(true)}>
                + New Room
              </button>
            </div>
          )}

          {/* Direct Messages section */}
          {rooms.filter(r => r.isDm).length > 0 && (
            <div className="sidebar-section" style={{ borderTop: '1px solid var(--border)', marginTop: 4 }}>
              <div className="sidebar-section-title">Direct Messages</div>
              {rooms.filter(r => r.isDm).map((room) => (
                <div
                  key={room.id}
                  className={`room-item ${activeRoomId === room.id ? 'active' : ''}`}
                  onClick={() => setActiveRoomId(room.id)}
                >
                  <div className="room-item-name">
                    <span style={{ color: 'var(--text-muted)', fontSize: 13 }}>@</span>
                    <span>{room.dmPartnerName ?? room.name}</span>
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                    {drafts.has(room.id) && activeRoomId !== room.id && (
                      <span title="Draft saved" style={{ fontSize: 11, color: 'var(--text-muted)' }}>✏️</span>
                    )}
                    {room.unreadCount > 0 && (
                      <span className="unread-badge">{room.unreadCount}</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="sidebar-section" style={{ borderTop: '1px solid var(--border)', marginTop: 4 }}>
            <div className="sidebar-section-title">Presence</div>
            {allUsers.length === 0 && onlineUsers.length === 0 ? (
              <div style={{ padding: '8px 16px', fontSize: 13, color: 'var(--text-muted)' }}>No users yet</div>
            ) : (() => {
                const merged = new Map<number, User>();
                for (const u of allUsers) merged.set(u.id, u);
                for (const u of onlineUsers) {
                  if (!merged.has(u.id)) merged.set(u.id, u);
                  else merged.set(u.id, { ...merged.get(u.id)!, online: u.online, status: u.status ?? merged.get(u.id)!.status });
                }
                const order: Record<string, number> = { online: 0, away: 1, dnd: 2, invisible: 3, offline: 4 };
                const sorted = Array.from(merged.values()).sort((a, b) => {
                  const sa = a.id === currentUser.id ? myStatus : (a.status ?? 'offline');
                  const sb = b.id === currentUser.id ? myStatus : (b.status ?? 'offline');
                  return (order[sa] ?? 4) - (order[sb] ?? 4);
                });
                return sorted.map((u) => {
                  const isMe = u.id === currentUser.id;
                  const effectiveStatus: UserStatus = isMe ? myStatus : (u.status ?? (u.online ? 'online' : 'offline'));
                  const isOffline = effectiveStatus === 'offline' || effectiveStatus === 'invisible';
                  return (
                    <div key={u.id} className="user-item" style={{ flexDirection: 'column', alignItems: 'flex-start', padding: '6px 16px' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6, width: '100%' }}>
                        <span style={{ width: 8, height: 8, borderRadius: '50%', background: statusColor(effectiveStatus), flexShrink: 0 }} />
                        <span style={{ fontSize: 13, color: isMe ? 'var(--accent)' : 'var(--text)', flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {u.name}{isMe ? ' (you)' : ''}
                        </span>
                        {!isMe && (
                          <button
                            className="btn btn-ghost btn-sm"
                            style={{ fontSize: 10, padding: '1px 5px', flexShrink: 0 }}
                            onClick={() => handleOpenDM(u.id)}
                            title={`Message ${u.name}`}
                          >
                            DM
                          </button>
                        )}
                      </div>
                      {isOffline && u.lastSeen && !isMe && (
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', paddingLeft: 14 }}>
                          Active {formatLastSeen(u.lastSeen)}
                        </div>
                      )}
                    </div>
                  );
                });
              })()
            }
          </div>
        </div>

        <div className="sidebar-user-info" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: 6, padding: '10px 16px' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
            <span style={{ width: 8, height: 8, borderRadius: '50%', background: statusColor(myStatus), flexShrink: 0 }} />
            <span className="user-name" style={{ flex: 1 }}>{currentUser.name}</span>
            {!connected && <span style={{ fontSize: 11, color: 'var(--warning)' }}>●</span>}
          </div>
          <select
            style={{ fontSize: 11, background: 'var(--surface)', color: 'var(--text-muted)', border: '1px solid var(--border)', borderRadius: 4, padding: '2px 4px', width: '100%', cursor: 'pointer' }}
            value={myStatus}
            onChange={(e) => handleSetStatus(e.target.value as UserStatus)}
          >
            <option value="online">● Online</option>
            <option value="away">● Away</option>
            <option value="dnd">● Do Not Disturb</option>
            <option value="invisible">● Invisible</option>
          </select>
        </div>
      </aside>

      {/* ── Invitations Panel ── */}
      {showInvitationsPanel && (
        <div className="modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) setShowInvitationsPanel(false); }}>
          <div className="modal">
            <h2>Pending Invitations</h2>
            {invitations.length === 0 ? (
              <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>No pending invitations</p>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                {invitations.map((inv) => (
                  <div key={inv.id} style={{ background: 'var(--surface)', padding: '10px 12px', borderRadius: 6, fontSize: 13 }}>
                    <div style={{ fontWeight: 600, marginBottom: 2 }}>#{inv.roomName}</div>
                    <div style={{ color: 'var(--text-muted)', fontSize: 12, marginBottom: 8 }}>
                      Invited by {inv.inviterName}
                    </div>
                    <div style={{ display: 'flex', gap: 6 }}>
                      <button className="btn btn-primary btn-sm" onClick={() => handleAcceptInvitation(inv.id)}>Accept</button>
                      <button className="btn btn-ghost btn-sm" style={{ color: 'var(--danger)' }} onClick={() => handleDeclineInvitation(inv.id)}>Decline</button>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <button className="btn btn-ghost" style={{ marginTop: 12 }} onClick={() => setShowInvitationsPanel(false)}>Close</button>
          </div>
        </div>
      )}

      {/* ── Invite User Modal ── */}
      {showInviteUserModal && activeRoomId && (
        <div className="modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) { setShowInviteUserModal(false); setInviteError(''); setInviteSuccess(''); setInviteUsername(''); } }}>
          <div className="modal">
            <h2>Invite User</h2>
            <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>
              Invite someone to #{rooms.find((r) => r.id === activeRoomId)?.name}
            </p>
            <form onSubmit={handleInviteUser} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div className="form-field">
                <label>Username</label>
                <input
                  className="text-input"
                  type="text"
                  placeholder="Enter exact username..."
                  value={inviteUsername}
                  onChange={(e) => setInviteUsername(e.target.value)}
                  maxLength={30}
                  autoFocus
                />
                {inviteError && <span className="error-msg">{inviteError}</span>}
                {inviteSuccess && <span style={{ color: 'var(--success)', fontSize: 12 }}>{inviteSuccess}</span>}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <button className="btn btn-primary" type="submit" disabled={!inviteUsername.trim()}>Invite</button>
                <button className="btn btn-ghost" type="button" onClick={() => { setShowInviteUserModal(false); setInviteError(''); setInviteSuccess(''); setInviteUsername(''); }}>Cancel</button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ── Schedule Modal ── */}
      {showScheduleModal && activeRoomId && (
        <div className="modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) setShowScheduleModal(false); }}>
          <div className="modal">
            <h2>Schedule Message</h2>
            <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>
              in #{rooms.find((r) => r.id === activeRoomId)?.name}
            </p>
            <form onSubmit={handleScheduleMessage} style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div className="form-field">
                <label>Message</label>
                <input
                  className="text-input"
                  type="text"
                  placeholder="Type your message..."
                  value={scheduleContent}
                  onChange={(e) => setScheduleContent(e.target.value)}
                  maxLength={2000}
                  autoFocus
                />
              </div>
              <div className="form-field">
                <label>Send at</label>
                <input
                  className="text-input"
                  type="datetime-local"
                  value={scheduleTime}
                  onChange={(e) => setScheduleTime(e.target.value)}
                  min={(() => { const d = new Date(Date.now() + 60000); const p = (n: number) => n.toString().padStart(2, '0'); return `${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`; })()}
                />
              </div>
              {scheduleError && <span className="error-msg">{scheduleError}</span>}
              <div style={{ display: 'flex', gap: 8 }}>
                <button className="btn btn-primary" type="submit" disabled={!scheduleContent.trim() || !scheduleTime}>
                  Schedule
                </button>
                <button className="btn btn-ghost" type="button" onClick={() => { setShowScheduleModal(false); setScheduleError(''); }}>
                  Cancel
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ── Edit History Modal ── */}
      {historyMessageId !== null && (
        <div className="modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) setHistoryMessageId(null); }}>
          <div className="modal">
            <h2>Edit History</h2>
            {editHistory.length === 0 ? (
              <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>No edit history available.</p>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                {editHistory.map((entry) => (
                  <div key={entry.id} style={{ background: 'var(--surface)', padding: '8px 12px', borderRadius: 6, fontSize: 13 }}>
                    <div style={{ color: 'var(--text-muted)', fontSize: 11, marginBottom: 4 }}>
                      {new Date(entry.editedAt).toLocaleString()}
                    </div>
                    <div style={{ color: 'var(--text)' }}>{entry.content}</div>
                  </div>
                ))}
              </div>
            )}
            <button className="btn btn-ghost" style={{ marginTop: 12 }} onClick={() => setHistoryMessageId(null)}>Close</button>
          </div>
        </div>
      )}

      {/* ── Main Area ── */}
      <main className="main-area">
        {!connected && (
          <div className="connection-banner">Reconnecting…</div>
        )}

        {!activeRoom ? (
          <div className="empty-state">
            <h2>PostgreSQL Chat</h2>
            {kickedNotice ? (
              <p style={{ color: 'var(--danger)' }}>{kickedNotice}</p>
            ) : (
              <p>Select a room from the sidebar to start chatting,<br />or create a new one.</p>
            )}
            {rooms.length === 0 && (
              <button className="btn btn-primary" onClick={() => setShowCreateRoom(true)}>
                Create a Room
              </button>
            )}
          </div>
        ) : (
          <>
            <div className="room-header">
              <span className="room-header-name">
                {activeRoom.isDm ? `@ ${activeRoom.dmPartnerName ?? activeRoom.name}` : `${activeRoom.isPrivate ? '🔒 ' : '# '}${activeRoom.name}`}
              </span>
              <div className="room-header-actions">
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={handleOpenMembersPanel}
                >
                  Members
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setShowScheduledPanel((p) => !p)}
                  title="Scheduled messages"
                >
                  ⏰{scheduledMessages.length > 0 && <span className="unread-badge" style={{ marginLeft: 4 }}>{scheduledMessages.length}</span>}
                </button>
                {activeRoom.joined && (
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => handleLeaveRoom(activeRoom.id)}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            <div
              className="messages-container"
              ref={messagesContainerRef}
              onScroll={handleScroll}
              style={{ position: 'relative' }}
            >
              {messagesLoading ? (
                <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}>
                  <div className="connecting-state">
                    <div className="spinner" />
                    Loading messages…
                  </div>
                </div>
              ) : messages.length === 0 ? (
                <div className="empty-state" style={{ flex: 'none', marginTop: 40 }}>
                  <p>No messages yet. Say hello!</p>
                </div>
              ) : (
                groupedMessages.map(({ msgs }) => {
                  const first = msgs[0];
                  const isOwn = first.userId === currentUser.id;
                  return (
                    <div key={first.id} className="message-group">
                      <div className="message-header">
                        <span
                          className="message-sender"
                          style={{ color: isOwn ? 'var(--warning)' : 'var(--accent)' }}
                        >
                          {first.userName}
                        </span>
                        <span className="message-time">{formatTime(first.createdAt)}</span>
                      </div>
                      {msgs.map((msg) => {
                        // Group reactions by emoji
                        const reactionGroups: { emoji: string; users: string[]; hasMe: boolean }[] = [];
                        for (const r of msg.reactions) {
                          const g = reactionGroups.find((g) => g.emoji === r.emoji);
                          if (g) {
                            g.users.push(r.userName);
                            if (r.userId === currentUser.id) g.hasMe = true;
                          } else {
                            reactionGroups.push({ emoji: r.emoji, users: [r.userName], hasMe: r.userId === currentUser.id });
                          }
                        }
                        return (
                          <div key={msg.id} className="message-wrapper">
                            {editingMessageId === msg.id ? (
                              <form
                                className="edit-form"
                                onSubmit={(e) => handleSaveEdit(e, msg.id)}
                                onKeyDown={(e) => { if (e.key === 'Escape') { setEditingMessageId(null); setEditInput(''); } }}
                              >
                                <input
                                  className="text-input edit-input"
                                  type="text"
                                  value={editInput}
                                  onChange={(e) => setEditInput(e.target.value)}
                                  maxLength={2000}
                                  autoFocus
                                />
                                <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
                                  <button className="btn btn-primary btn-sm" type="submit" disabled={!editInput.trim()}>Save</button>
                                  <button className="btn btn-ghost btn-sm" type="button" onClick={() => { setEditingMessageId(null); setEditInput(''); }}>Cancel</button>
                                </div>
                              </form>
                            ) : (
                            <div className={`message-row ${isOwn ? 'own' : ''}`}>
                              <div className="message-content">
                                {msg.content}
                                {msg.editedAt && (
                                  <span
                                    className="edited-badge"
                                    title="Click to see edit history"
                                    style={{ cursor: 'pointer' }}
                                    onClick={() => handleShowHistory(msg.id)}
                                  >
                                    (edited)
                                  </span>
                                )}
                                {msg.expiresAt && (
                                  <span className="ephemeral-badge" title="This message will disappear">
                                    ⏳ {formatCountdown(msg.expiresAt)}
                                  </span>
                                )}
                              </div>
                              <div className="message-actions">
                                <button
                                  className="reaction-btn"
                                  onClick={() => handleOpenThread(msg.id)}
                                  title="Reply in thread"
                                >
                                  Reply
                                </button>
                                {isOwn && (
                                  <button
                                    className="reaction-btn"
                                    onClick={() => handleStartEdit(msg)}
                                    title="Edit message"
                                  >
                                    Edit
                                  </button>
                                )}
                                {(['👍', '❤️', '😂', '😮', '😢'] as const).map((emoji) => (
                                  <button
                                    key={emoji}
                                    className="reaction-btn"
                                    onClick={() => handleToggleReaction(msg.id, emoji)}
                                    title={`React with ${emoji}`}
                                  >
                                    {emoji}
                                  </button>
                                ))}
                              </div>
                            </div>
                            )}
                            {reactionGroups.length > 0 && (
                              <div className="reaction-row">
                                {reactionGroups.map((g) => (
                                  <button
                                    key={g.emoji}
                                    className={`reaction-count ${g.hasMe ? 'reacted' : ''}`}
                                    onClick={() => handleToggleReaction(msg.id, g.emoji)}
                                    title={g.users.join(', ')}
                                  >
                                    {g.emoji} {g.users.length}
                                  </button>
                                ))}
                              </div>
                            )}
                            {(msg.replyCount ?? 0) > 0 && (
                              <button
                                className="reaction-count"
                                style={{ marginTop: 4, fontSize: 12 }}
                                onClick={() => handleOpenThread(msg.id)}
                                title="View thread"
                              >
                                💬 {msg.replyCount} {msg.replyCount === 1 ? 'reply' : 'replies'}
                              </button>
                            )}
                            {msg.readBy.length > 0 && (
                              <div className="read-receipts">
                                <span className="read-receipts-icon">✓✓</span>
                                Seen by {msg.readBy.map((r) => r.userName).join(', ')}
                              </div>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {isScrolledUp && (
              <button
                className="scroll-to-bottom"
                onClick={() => { scrollToBottom(); setIsScrolledUp(false); }}
              >
                ↓ New messages
              </button>
            )}

            <div className="typing-indicator">
              {typingText && (
                <>
                  {typingText}
                  <span className="typing-dots">
                    <span /><span /><span />
                  </span>
                </>
              )}
            </div>

            <form className="input-bar" onSubmit={handleSendMessage}>
              <input
                className="message-input"
                type="text"
                placeholder={`${activeRoom.isDm ? `Message ${activeRoom.dmPartnerName ?? activeRoom.name}` : `Message #${activeRoom.name}`}${ephemeralDuration > 0 ? ' (ephemeral)' : ''}`}
                value={messageInput}
                onChange={handleMessageInput}
                maxLength={2000}
                autoComplete="off"
              />
              <select
                className="ephemeral-select"
                value={ephemeralDuration}
                onChange={(e) => setEphemeralDuration(parseInt(e.target.value))}
                title="Disappears after…"
              >
                <option value={0}>No expiry</option>
                <option value={60000}>1 min</option>
                <option value={300000}>5 min</option>
                <option value={3600000}>1 hour</option>
              </select>
              <button
                className="btn btn-ghost btn-sm"
                type="button"
                title="Schedule message"
                onClick={() => { setShowScheduleModal(true); setScheduleError(''); }}
              >
                ⏰
              </button>
              <button
                className="btn btn-primary"
                type="submit"
                disabled={!messageInput.trim()}
              >
                Send
              </button>
            </form>

            {/* Members panel */}
            {showMembersPanel && (
              <div className="scheduled-panel">
                <div className="scheduled-panel-header">
                  <span>Members</span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    {currentUserIsAdmin && activeRoom?.isPrivate && (
                      <button
                        className="btn btn-ghost btn-sm"
                        style={{ fontSize: 11 }}
                        onClick={() => { setShowInviteUserModal(true); setInviteError(''); setInviteSuccess(''); }}
                        title="Invite user"
                      >
                        + Invite
                      </button>
                    )}
                    <button className="btn btn-ghost btn-sm" onClick={() => setShowMembersPanel(false)}>✕</button>
                  </div>
                </div>
                {roomMembers.length === 0 ? (
                  <div style={{ padding: '12px 16px', color: 'var(--text-muted)', fontSize: 13 }}>No members</div>
                ) : (
                  roomMembers.map((member) => (
                    <div key={member.userId} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
                      <span style={{ flex: 1, fontSize: 13 }}>{member.name}</span>
                      {member.isAdmin && (
                        <span style={{ fontSize: 10, background: 'var(--primary)', color: '#fff', padding: '2px 6px', borderRadius: 4 }}>Admin</span>
                      )}
                      {currentUserIsAdmin && member.userId !== currentUser.id && !member.isAdmin && (
                        <>
                          <button
                            className="btn btn-ghost btn-sm"
                            style={{ fontSize: 11, color: 'var(--danger)' }}
                            onClick={() => handleKickUser(member.userId)}
                          >
                            Kick
                          </button>
                          <button
                            className="btn btn-ghost btn-sm"
                            style={{ fontSize: 11 }}
                            onClick={() => handlePromoteUser(member.userId)}
                          >
                            Promote
                          </button>
                        </>
                      )}
                    </div>
                  ))
                )}
              </div>
            )}

            {/* Thread panel */}
            {activeThreadParentId !== null && (
              <div className="scheduled-panel" style={{ width: 360, display: 'flex', flexDirection: 'column', maxHeight: '80vh' }}>
                <div className="scheduled-panel-header">
                  <span>Thread</span>
                  <button className="btn btn-ghost btn-sm" onClick={handleCloseThread}>✕</button>
                </div>
                {threadLoading ? (
                  <div style={{ padding: '16px', color: 'var(--text-muted)', fontSize: 13 }}>Loading thread…</div>
                ) : (
                  <>
                    {threadParentMsg && (
                      <div style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', background: 'var(--bg)' }}>
                        <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--accent)', marginBottom: 4 }}>{threadParentMsg.userName}</div>
                        <div style={{ fontSize: 13, color: 'var(--text)' }}>{threadParentMsg.content}</div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>
                          {threadMessages.length} {threadMessages.length === 1 ? 'reply' : 'replies'}
                        </div>
                      </div>
                    )}
                    <div style={{ flex: 1, overflowY: 'auto', padding: '8px 0' }}>
                      {threadMessages.length === 0 ? (
                        <div style={{ padding: '12px 16px', color: 'var(--text-muted)', fontSize: 13 }}>No replies yet. Start the conversation!</div>
                      ) : (
                        threadMessages.map((msg) => (
                          <div key={msg.id} style={{ padding: '6px 16px', borderBottom: '1px solid var(--border)' }}>
                            <div style={{ display: 'flex', gap: 8, alignItems: 'baseline', marginBottom: 2 }}>
                              <span style={{ fontSize: 12, fontWeight: 600, color: msg.userId === currentUser.id ? 'var(--warning)' : 'var(--accent)' }}>
                                {msg.userName}
                              </span>
                              <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{formatTime(msg.createdAt)}</span>
                            </div>
                            <div style={{ fontSize: 13, color: 'var(--text)' }}>
                              {msg.content}
                              {msg.editedAt && <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 4 }}>(edited)</span>}
                            </div>
                          </div>
                        ))
                      )}
                    </div>
                    <form onSubmit={handleSendThreadReply} style={{ padding: '8px 16px', borderTop: '1px solid var(--border)', display: 'flex', gap: 6 }}>
                      <input
                        className="message-input"
                        style={{ flex: 1, fontSize: 13, padding: '6px 10px' }}
                        type="text"
                        placeholder="Reply in thread…"
                        value={threadInput}
                        onChange={(e) => setThreadInput(e.target.value)}
                        maxLength={2000}
                        autoComplete="off"
                      />
                      <button className="btn btn-primary btn-sm" type="submit" disabled={!threadInput.trim()}>
                        Reply
                      </button>
                    </form>
                  </>
                )}
              </div>
            )}

            {/* Scheduled messages panel */}
            {showScheduledPanel && (
              <div className="scheduled-panel">
                <div className="scheduled-panel-header">
                  <span>Scheduled Messages</span>
                  <button className="btn btn-ghost btn-sm" onClick={() => setShowScheduledPanel(false)}>✕</button>
                </div>
                {scheduledMessages.length === 0 ? (
                  <div style={{ padding: '12px 16px', color: 'var(--text-muted)', fontSize: 13 }}>No scheduled messages</div>
                ) : (
                  scheduledMessages.map((s) => (
                    <div key={s.id} className="scheduled-item">
                      <div className="scheduled-item-meta">
                        <span style={{ color: 'var(--accent)', fontSize: 12 }}>#{s.roomName}</span>
                        <span style={{ color: 'var(--text-muted)', fontSize: 11 }}>
                          {new Date(s.scheduledFor).toLocaleString()}
                        </span>
                      </div>
                      <div className="scheduled-item-content">{s.content}</div>
                      <button
                        className="btn btn-ghost btn-sm"
                        style={{ color: 'var(--danger)', fontSize: 11, marginTop: 4 }}
                        onClick={() => handleCancelScheduled(s.id)}
                      >
                        Cancel
                      </button>
                    </div>
                  ))
                )}
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}
