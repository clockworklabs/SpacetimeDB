import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

interface User {
  id: number;
  username: string;
  isAnonymous?: boolean;
  status?: string;
  lastActiveAt?: string;
}

interface RoomMember {
  userId: number;
  username: string;
  role: string;
  status?: string;
}

interface Room {
  id: number;
  name: string;
  isPrivate: boolean;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  username: string;
  content: string;
  createdAt: string;
  expiresAt?: string | null;
  editedAt?: string | null;
  replyCount?: number;
  parentMessageId?: number | null;
}

interface MessageEdit {
  id: number;
  previousContent: string;
  editedAt: string;
  username: string;
}

interface ReadReceiptMap {
  [messageId: number]: { userId: number; username: string }[];
}

interface Reaction {
  messageId: number;
  userId: number;
  emoji: string;
  username: string;
}

type ReactionMap = Record<number, Reaction[]>;

interface ScheduledMessage {
  id: number;
  roomId: number;
  content: string;
  scheduledAt: string;
  createdAt: string;
  roomName: string;
}

interface PendingInvite {
  inviteId: string;
  roomId: number;
  roomName: string;
  inviterUsername: string;
}

function App() {
  const [connected, setConnected] = useState(false);
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [loginName, setLoginName] = useState('');
  const [loginError, setLoginError] = useState('');
  const [showRegisterModal, setShowRegisterModal] = useState(false);
  const [registerName, setRegisterName] = useState('');
  const [registerError, setRegisterError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<number | null>(null);
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomIsPrivate, setNewRoomIsPrivate] = useState(false);

  const [messages, setMessages] = useState<Message[]>([]);
  const [messageInput, setMessageInput] = useState('');

  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [knownUsers, setKnownUsers] = useState<Record<number, string>>({});
  const [typingUsers, setTypingUsers] = useState<Map<number, string>>(new Map());
  const [readReceipts, setReadReceipts] = useState<ReadReceiptMap>({});
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [joinedRooms, setJoinedRooms] = useState<Set<number>>(new Set());
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [showSchedulePanel, setShowSchedulePanel] = useState(false);
  const [scheduleInput, setScheduleInput] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [scheduleError, setScheduleError] = useState('');
  const [ephemeralSeconds, setEphemeralSeconds] = useState<number | null>(null);
  const [, setNow] = useState(Date.now());
  const [reactions, setReactions] = useState<ReactionMap>({});
  const [hoveredMessage, setHoveredMessage] = useState<number | null>(null);
  const [editingMessageId, setEditingMessageId] = useState<number | null>(null);
  const [editInput, setEditInput] = useState('');
  const [editHistoryMessageId, setEditHistoryMessageIdState] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);
  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [kickedNotice, setKickedNotice] = useState<string | null>(null);
  // presence: userId -> { status, lastActiveAt }
  const [userPresence, setUserPresence] = useState<Record<number, { status: string; lastActiveAt: string }>>({});
  const [myStatus, setMyStatus] = useState<string>('online');
  const lastActivityRef = useRef<number>(Date.now());

  // Threading state
  const [threadOpenMessageId, setThreadOpenMessageIdState] = useState<number | null>(null);
  const [threadParentMsg, setThreadParentMsg] = useState<Message | null>(null);
  const [threadReplies, setThreadReplies] = useState<Message[]>([]);
  const [threadReplyInput, setThreadReplyInput] = useState('');
  const threadOpenMessageIdRef = useRef<number | null>(null);
  const setThreadOpenMessageId = (id: number | null) => {
    threadOpenMessageIdRef.current = id;
    setThreadOpenMessageIdState(id);
  };

  const setEditHistoryMessageId = (id: number | null) => {
    editHistoryMessageIdRef.current = id;
    setEditHistoryMessageIdState(id);
  };

  const [inviteUsername, setInviteUsername] = useState('');
  const [inviteError, setInviteError] = useState('');
  const [pendingInvites, setPendingInvites] = useState<PendingInvite[]>([]);

  const socketRef = useRef<Socket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);
  const editHistoryMessageIdRef = useRef<number | null>(null);
  const currentRoomIdRef = useRef<number | null>(null);
  const [showScrollBtn, setShowScrollBtn] = useState(false);
  const [roomActivity, setRoomActivity] = useState<Record<number, { level: string; recentCount: number }>>({});
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Socket setup ───────────────────────────────────────────────────────────
  useEffect(() => {
    const socket = io({ path: '/socket.io' });
    socketRef.current = socket;

    socket.on('connect', () => setConnected(true));
    socket.on('disconnect', () => setConnected(false));

    socket.on('online_users', (users: User[]) => {
      setOnlineUsers(users);
      // Populate presence map from online_users
      setUserPresence(prev => {
        const next = { ...prev };
        for (const u of users) {
          if (u.status !== undefined) {
            next[u.id] = { status: u.status, lastActiveAt: u.lastActiveAt || new Date().toISOString() };
          }
        }
        return next;
      });
      // Persist usernames so DM room names survive offline transitions
      setKnownUsers(prev => {
        const next = { ...prev };
        for (const u of users) next[u.id] = u.username;
        return next;
      });
    });

    socket.on('user_presence_update', (data: { userId: number; username: string; status: string; lastActiveAt: string }) => {
      setUserPresence(prev => ({
        ...prev,
        [data.userId]: { status: data.status, lastActiveAt: data.lastActiveAt },
      }));
      if (currentUser && data.userId === currentUser.id) {
        setMyStatus(data.status);
      }
    });

    socket.on('room_created', (room: Room) => {
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
    });

    socket.on('new_message', (msg: Message) => {
      setCurrentRoomId(current => {
        if (current === msg.roomId) {
          setMessages(prev => {
            if (prev.find(m => m.id === msg.id)) return prev;
            return [...prev, msg];
          });
        } else {
          setUnreadCounts(counts => ({
            ...counts,
            [msg.roomId]: (counts[msg.roomId] || 0) + 1,
          }));
        }
        return current;
      });
    });

    socket.on('user_typing', (data: { userId: number; username: string; roomId: number }) => {
      setCurrentRoomId(current => {
        if (current === data.roomId) {
          setTypingUsers(prev => {
            const next = new Map(prev);
            next.set(data.userId, data.username);
            return next;
          });
        }
        return current;
      });
    });

    socket.on('user_stopped_typing', (data: { userId: number; roomId: number }) => {
      setCurrentRoomId(current => {
        if (current === data.roomId) {
          setTypingUsers(prev => {
            const next = new Map(prev);
            next.delete(data.userId);
            return next;
          });
        }
        return current;
      });
    });

    socket.on('read_receipt_update', (data: { messageId: number; readers: { userId: number; username: string }[] }) => {
      setReadReceipts(prev => ({ ...prev, [data.messageId]: data.readers }));
    });

    socket.on('scheduled_message_sent', (data: { id: number }) => {
      setScheduledMessages(prev => prev.filter(m => m.id !== data.id));
    });

    socket.on('message_deleted', (data: { messageId: number }) => {
      setMessages(prev => prev.filter(m => m.id !== data.messageId));
    });

    socket.on('reaction_update', (data: { messageId: number; reactions: Reaction[] }) => {
      setReactions(prev => ({ ...prev, [data.messageId]: data.reactions }));
    });

    socket.on('message_edited', (msg: Message) => {
      setMessages(prev => prev.map(m => m.id === msg.id ? { ...m, content: msg.content, editedAt: msg.editedAt } : m));
      if (editHistoryMessageIdRef.current === msg.id) {
        fetch(`/api/messages/${msg.id}/edits`)
          .then(r => r.json())
          .then((edits: MessageEdit[]) => setEditHistory(edits))
          .catch(() => {});
      }
    });

    socket.on('room_activity_update', (data: { roomId: number; level: string; recentCount: number }) => {
      setRoomActivity(prev => ({ ...prev, [data.roomId]: { level: data.level, recentCount: data.recentCount } }));
    });

    socket.on('draft_update', (data: { roomId: number; content: string }) => {
      setDrafts(prev => ({ ...prev, [data.roomId]: data.content }));
      // If the user is currently in this room and input is empty, update it
      setCurrentRoomId(current => {
        if (current === data.roomId) {
          setMessageInput(prev => prev === '' ? data.content : prev);
        }
        return current;
      });
    });

    socket.on('new_reply', (reply: Message) => {
      // Update reply count on parent message
      setMessages(prev => prev.map(m =>
        m.id === reply.parentMessageId
          ? { ...m, replyCount: (m.replyCount || 0) + 1 }
          : m
      ));
      // If thread panel is open for this parent, append the reply
      if (threadOpenMessageIdRef.current === reply.parentMessageId) {
        setThreadReplies(prev => {
          if (prev.find(r => r.id === reply.id)) return prev;
          return [...prev, reply];
        });
      }
    });

    socket.on('kicked_from_room', (data: { roomId: number; banned?: boolean }) => {
      setCurrentRoomId(current => {
        if (current === data.roomId) {
          setMessages([]);
          setReadReceipts({});
          setReactions({});
          setTypingUsers(new Map());
          setRoomMembers([]);
          setKickedNotice(data.banned ? 'You have been banned from this room.' : 'You have been kicked from this room.');
        }
        return null;
      });
      setJoinedRooms(prev => {
        const next = new Set(prev);
        next.delete(data.roomId);
        return next;
      });
    });

    socket.on('member_added', (data: { userId: number; roomId: number; role: string; username: string }) => {
      if (data.roomId !== currentRoomIdRef.current) return;
      setRoomMembers(prev => {
        if (prev.find(m => m.userId === data.userId)) return prev;
        return [...prev, { userId: data.userId, username: data.username, role: data.role }];
      });
    });

    socket.on('member_removed', (data: { userId: number; roomId: number }) => {
      if (data.roomId !== currentRoomIdRef.current) return;
      setRoomMembers(prev => prev.filter(m => m.userId !== data.userId));
    });

    socket.on('member_role_changed', (data: { userId: number; role: string; username: string; roomId: number }) => {
      setRoomMembers(prev => prev.map(m => m.userId === data.userId ? { ...m, role: data.role } : m));
    });

    socket.on('room_invite_received', (invite: PendingInvite) => {
      setPendingInvites(prev => {
        if (prev.find(i => i.inviteId === invite.inviteId)) return prev;
        return [...prev, invite];
      });
    });

    socket.on('room_invited', (room: Room) => {
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
      // Subscribe to the room for real-time messages
      socket.emit('join_room', room.id);
      setJoinedRooms(prev => new Set([...prev, room.id]));
    });

    socket.on('user_identity_updated', (data: { userId: number; oldUsername: string; newUsername: string; isAnonymous: boolean }) => {
      // Update messages that reference the old username
      setMessages(prev => prev.map(m =>
        m.userId === data.userId ? { ...m, username: data.newUsername } : m
      ));
      setKnownUsers(prev => ({ ...prev, [data.userId]: data.newUsername }));
      // Update current user if it's us
      setCurrentUser(prev => {
        if (!prev || prev.id !== data.userId) return prev;
        return { ...prev, username: data.newUsername, isAnonymous: data.isAnonymous };
      });
    });

    return () => {
      socket.disconnect();
    };
  }, []);

  // Keep currentRoomIdRef in sync with currentRoomId state
  useEffect(() => {
    currentRoomIdRef.current = currentRoomId;
  }, [currentRoomId]);

  // Poll room members every 3 seconds to keep list live
  useEffect(() => {
    if (!currentRoomId) return;
    const interval = setInterval(async () => {
      try {
        const res = await fetch(`/api/rooms/${currentRoomId}/members`);
        if (res.ok) {
          const members: RoomMember[] = await res.json();
          setRoomMembers(members);
          setUserPresence(prev => {
            const next = { ...prev };
            for (const m of members) {
              if (m.status && !next[m.userId]) {
                next[m.userId] = { status: m.status, lastActiveAt: new Date().toISOString() };
              }
            }
            return next;
          });
        }
      } catch {}
    }, 3000);
    return () => clearInterval(interval);
  }, [currentRoomId]);

  // Countdown ticker for ephemeral messages
  useEffect(() => {
    const hasEphemeral = messages.some(m => m.expiresAt);
    if (!hasEphemeral) return;
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, [messages]);

  // ── Login ──────────────────────────────────────────────────────────────────
  const handleLogin = async () => {
    const name = loginName.trim();
    if (!name) { setLoginError('Enter a display name'); return; }
    if (name.length > 32) { setLoginError('Name too long (max 32 chars)'); return; }
    try {
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
      socketRef.current?.emit('user_connected', { userId: user.id, username: user.username });
      loadRooms(user.id);
      loadScheduledMessages(user.id);
      loadDrafts(user.id);
    } catch {
      setLoginError('Connection error');
    }
  };

  const handleJoinAsGuest = async () => {
    try {
      const res = await fetch('/api/users/anonymous', { method: 'POST' });
      if (!res.ok) {
        const err = await res.json();
        setLoginError(err.error || 'Failed to join as guest');
        return;
      }
      const user: User = await res.json();
      setCurrentUser(user);
      socketRef.current?.emit('user_connected', { userId: user.id, username: user.username });
      loadRooms(user.id);
      loadScheduledMessages(user.id);
      loadDrafts(user.id);
    } catch {
      setLoginError('Connection error');
    }
  };

  const handleRegister = async () => {
    if (!currentUser) return;
    const name = registerName.trim();
    if (!name) { setRegisterError('Enter a username'); return; }
    if (name.length > 32) { setRegisterError('Name too long (max 32 chars)'); return; }
    try {
      const res = await fetch(`/api/users/${currentUser.id}/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username: name }),
      });
      if (!res.ok) {
        const err = await res.json();
        setRegisterError(err.error || 'Failed to register');
        return;
      }
      const updated: User = await res.json();
      setCurrentUser(updated);
      setShowRegisterModal(false);
      setRegisterName('');
      setRegisterError('');
      // Update socket user info
      socketRef.current?.emit('user_connected', { userId: updated.id, username: updated.username });
    } catch {
      setRegisterError('Connection error');
    }
  };

  const handleStatusChange = async (status: string) => {
    if (!currentUser) return;
    setMyStatus(status);
    lastActivityRef.current = Date.now();
    await fetch(`/api/users/${currentUser.id}/status`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status }),
    });
  };

  // Auto-away after 5 minutes of inactivity (client-side mirror of server logic)
  useEffect(() => {
    if (!currentUser) return;
    const interval = setInterval(() => {
      const inactiveMs = Date.now() - lastActivityRef.current;
      if (inactiveMs > 5 * 60 * 1000 && myStatus === 'online') {
        handleStatusChange('away');
      }
    }, 60000);
    const resetActivity = () => {
      lastActivityRef.current = Date.now();
      if (myStatus === 'away') {
        handleStatusChange('online');
      }
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') resetActivity();
    };
    window.addEventListener('mousemove', resetActivity);
    window.addEventListener('keydown', resetActivity);
    window.addEventListener('click', resetActivity);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      clearInterval(interval);
      window.removeEventListener('mousemove', resetActivity);
      window.removeEventListener('keydown', resetActivity);
      window.removeEventListener('click', resetActivity);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [currentUser, myStatus]);

  // ── Rooms ──────────────────────────────────────────────────────────────────
  const loadRooms = async (userId: number) => {
    const [roomsRes, unreadRes, activityRes, usersRes] = await Promise.all([
      fetch(`/api/rooms?userId=${userId}`),
      fetch(`/api/users/${userId}/unread`),
      fetch(`/api/rooms/activity`),
      fetch(`/api/users`),
    ]);
    const roomsData: Room[] = await roomsRes.json();
    const unreadData: Record<number, number> = await unreadRes.json();
    const activityData: Record<number, { level: string; recentCount: number }> = await activityRes.json();
    const allUsers: User[] = await usersRes.json();
    setRooms(roomsData);
    setUnreadCounts(unreadData);
    setRoomActivity(activityData);
    // Seed knownUsers from all DB users so DM room names are always correct
    setKnownUsers(prev => {
      const next = { ...prev };
      for (const u of allUsers) next[u.id] = u.username;
      return next;
    });

    // Track which rooms user is a member of
    const memberRes = await Promise.all(
      roomsData.map(r => fetch(`/api/rooms/${r.id}/members`).then(res => res.json() as Promise<RoomMember[]>).then(members => ({ roomId: r.id, members })))
    );
    const joined = new Set<number>();
    for (const { roomId, members } of memberRes) {
      if (members.some((m: RoomMember) => m.userId === userId)) joined.add(roomId);
    }
    setJoinedRooms(joined);

    // Subscribe to all joined rooms via socket so new_message events arrive for unread tracking
    for (const roomId of joined) {
      socketRef.current?.emit('join_room', roomId);
    }
  };

  const loadDrafts = async (userId: number) => {
    try {
      const res = await fetch(`/api/users/${userId}/drafts`);
      if (!res.ok) return;
      const data: { roomId: number; content: string }[] = await res.json();
      const map: Record<number, string> = {};
      for (const d of data) map[d.roomId] = d.content;
      setDrafts(map);
    } catch {}
  };

  const loadScheduledMessages = async (userId: number) => {
    const res = await fetch(`/api/users/${userId}/scheduled-messages`);
    const data: ScheduledMessage[] = await res.json();
    setScheduledMessages(data);
  };

  const handleScheduleMessage = async () => {
    if (!currentUser || !currentRoomId || !scheduleInput.trim() || !scheduleTime) {
      setScheduleError('Fill in all fields');
      return;
    }
    const scheduledAt = new Date(scheduleTime);
    if (scheduledAt <= new Date()) {
      setScheduleError('Scheduled time must be in the future');
      return;
    }
    try {
      const res = await fetch(`/api/rooms/${currentRoomId}/scheduled-messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content: scheduleInput.trim(), scheduledAt: scheduledAt.toISOString() }),
      });
      if (!res.ok) {
        const err = await res.json();
        setScheduleError(err.error || 'Failed to schedule');
        return;
      }
      const newScheduled: ScheduledMessage = await res.json();
      // Fetch room name
      const room = rooms.find(r => r.id === currentRoomId);
      setScheduledMessages(prev => [...prev, { ...newScheduled, roomName: room?.name || '' }]);
      setScheduleInput('');
      setScheduleTime('');
      setScheduleError('');
      setShowSchedulePanel(false);
    } catch {
      setScheduleError('Connection error');
    }
  };

  const handleCancelScheduled = async (id: number) => {
    if (!currentUser) return;
    await fetch(`/api/scheduled-messages/${id}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    setScheduledMessages(prev => prev.filter(m => m.id !== id));
  };

  const handleCreateRoom = async () => {
    if (!newRoomName.trim() || !currentUser) return;
    try {
      const res = await fetch('/api/rooms', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: newRoomName.trim(), userId: currentUser.id, isPrivate: newRoomIsPrivate }),
      });
      if (res.ok) {
        setNewRoomName('');
        setNewRoomIsPrivate(false);
        const room = await res.json();
        setJoinedRooms(prev => new Set([...prev, room.id]));
      }
    } catch {}
  };

  const handleSelectRoom = async (roomId: number) => {
    if (!currentUser) return;
    if (currentRoomId === roomId) return;

    // Save current draft before switching rooms
    if (currentRoomId !== null && currentUser) {
      if (draftSaveTimerRef.current) {
        clearTimeout(draftSaveTimerRef.current);
        draftSaveTimerRef.current = null;
      }
      // messageInput captured via closure is fine — save it immediately
    }

    // Leave socket room
    if (currentRoomId !== null) {
      socketRef.current?.emit('leave_room', currentRoomId);
      // Clear typing for old room
      setTypingUsers(new Map());
    }

    setCurrentRoomId(roomId);
    setMessages([]);
    // Restore draft for the new room
    setMessageInput(drafts[roomId] || '');
    setReadReceipts({});
    setReactions({});
    setRoomMembers([]);
    setKickedNotice(null);

    // Join the room (DB + socket)
    if (!joinedRooms.has(roomId)) {
      const joinRes = await fetch(`/api/rooms/${roomId}/join`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      if (!joinRes.ok) {
        const err = await joinRes.json();
        setKickedNotice(err.error || 'Cannot join room');
        setCurrentRoomId(null);
        return;
      }
      setJoinedRooms(prev => new Set([...prev, roomId]));
    }
    socketRef.current?.emit('join_room', roomId);

    // Load messages
    const [msgsRes, receiptsRes, reactionsRes, membersRes] = await Promise.all([
      fetch(`/api/rooms/${roomId}/messages`),
      fetch(`/api/rooms/${roomId}/read-receipts?userId=${currentUser.id}`),
      fetch(`/api/rooms/${roomId}/reactions`),
      fetch(`/api/rooms/${roomId}/members`),
    ]);
    const msgsRaw: Message[] = await msgsRes.json();
    const msgs: Message[] = msgsRaw.map(m => ({ ...m, replyCount: m.replyCount != null ? parseInt(String(m.replyCount), 10) : 0 }));
    const receipts: ReadReceiptMap = await receiptsRes.json();
    const reactionsData: Reaction[] = await reactionsRes.json();
    const membersData: RoomMember[] = await membersRes.json();
    setMessages(msgs);
    setReadReceipts(receipts);
    setRoomMembers(membersData);
    // Pre-populate userPresence from DB status so dots show correctly before socket events arrive
    setUserPresence(prev => {
      const next = { ...prev };
      for (const m of membersData) {
        if (m.status && !next[m.userId]) {
          next[m.userId] = { status: m.status, lastActiveAt: new Date().toISOString() };
        }
      }
      return next;
    });
    // Group reactions by messageId
    const reactionsByMsg: ReactionMap = {};
    for (const r of reactionsData) {
      if (!reactionsByMsg[r.messageId]) reactionsByMsg[r.messageId] = [];
      reactionsByMsg[r.messageId].push(r);
    }
    setReactions(reactionsByMsg);
    setTypingUsers(new Map());

    // Mark last message as read
    if (msgs.length > 0) {
      const lastMsgId = msgs[msgs.length - 1].id;
      markRead(currentUser.id, roomId, lastMsgId);
    }

    // Clear unread count
    setUnreadCounts(counts => ({ ...counts, [roomId]: 0 }));
  };

  const handleLeaveRoom = async () => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/leave`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id }),
    });
    socketRef.current?.emit('leave_room', currentRoomId);
    setJoinedRooms(prev => {
      const next = new Set(prev);
      next.delete(currentRoomId);
      return next;
    });
    setCurrentRoomId(null);
    setMessages([]);
    setReadReceipts({});
    setReactions({});
    setTypingUsers(new Map());
    setRoomMembers([]);
  };

  const handleKickUser = async (targetUserId: number) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/kick`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    setRoomMembers(prev => prev.filter(m => m.userId !== targetUserId));
  };

  const handleBanUser = async (targetUserId: number) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/ban`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
    setRoomMembers(prev => prev.filter(m => m.userId !== targetUserId));
  };

  const handlePromoteUser = async (targetUserId: number) => {
    if (!currentUser || !currentRoomId) return;
    await fetch(`/api/rooms/${currentRoomId}/promote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ adminId: currentUser.id, targetUserId }),
    });
  };

  const handleStartDM = async (targetUserId: number) => {
    if (!currentUser) return;
    const res = await fetch('/api/dm', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, targetUserId }),
    });
    if (!res.ok) return;
    const room = await res.json();
    setRooms(prev => prev.some(r => r.id === room.id) ? prev : [...prev, room]);
    setCurrentRoomId(room.id);
  };

  const handleInviteUser = async () => {
    if (!currentUser || !currentRoomId || !inviteUsername.trim()) return;
    setInviteError('');
    try {
      const res = await fetch(`/api/rooms/${currentRoomId}/invite`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ adminId: currentUser.id, inviteeUsername: inviteUsername.trim() }),
      });
      if (!res.ok) {
        const err = await res.json();
        setInviteError(err.error || 'Failed to invite');
        return;
      }
      setInviteUsername('');
    } catch {
      setInviteError('Connection error');
    }
  };

  const handleAcceptInvite = async (invite: PendingInvite) => {
    if (!currentUser) return;
    try {
      const res = await fetch(`/api/invites/${invite.inviteId}/accept`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      if (res.ok) {
        setPendingInvites(prev => prev.filter(i => i.inviteId !== invite.inviteId));
      }
    } catch {}
  };

  const handleDeclineInvite = async (invite: PendingInvite) => {
    if (!currentUser) return;
    try {
      await fetch(`/api/invites/${invite.inviteId}/decline`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
    } catch {}
    setPendingInvites(prev => prev.filter(i => i.inviteId !== invite.inviteId));
  };

  const markRead = useCallback(async (userId: number, roomId: number, messageId: number) => {
    await fetch('/api/messages/read', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId, roomId, messageId }),
    });
  }, []);

  // ── Messaging ──────────────────────────────────────────────────────────────
  const handleSend = async () => {
    if (!currentUser || !currentRoomId || !messageInput.trim()) return;
    const content = messageInput.trim();
    setMessageInput('');
    stopTyping();
    // Cancel any pending draft save and clear draft for this room
    if (draftSaveTimerRef.current) {
      clearTimeout(draftSaveTimerRef.current);
      draftSaveTimerRef.current = null;
    }
    const roomId = currentRoomId;
    const userId = currentUser.id;
    setDrafts(prev => { const next = { ...prev }; delete next[roomId]; return next; });
    fetch(`/api/users/${userId}/drafts/${roomId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ content: '' }),
    }).catch(() => {});
    try {
      await fetch(`/api/rooms/${currentRoomId}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content, ...(ephemeralSeconds ? { expiresInSeconds: ephemeralSeconds } : {}) }),
      });
    } catch {}
  };

  const getEphemeralCountdown = (expiresAt: string): string => {
    const remaining = Math.max(0, Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000));
    if (remaining === 0) return 'Expiring...';
    if (remaining < 60) return `Disappears in ${remaining}s`;
    const mins = Math.floor(remaining / 60);
    const secs = remaining % 60;
    return `Disappears in ${mins}m ${secs}s`;
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // ── Typing indicators ──────────────────────────────────────────────────────
  const startTyping = useCallback(() => {
    if (!currentUser || !currentRoomId) return;
    if (!isTypingRef.current) {
      isTypingRef.current = true;
      socketRef.current?.emit('typing_start', { roomId: currentRoomId });
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => stopTyping(), 3000);
  }, [currentUser, currentRoomId]);

  const stopTyping = useCallback(() => {
    if (isTypingRef.current && currentRoomId) {
      isTypingRef.current = false;
      socketRef.current?.emit('typing_stop', { roomId: currentRoomId });
    }
    if (typingTimerRef.current) {
      clearTimeout(typingTimerRef.current);
      typingTimerRef.current = null;
    }
  }, [currentRoomId]);

  const saveDraft = useCallback((content: string, roomId: number, userId: number) => {
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    draftSaveTimerRef.current = setTimeout(() => {
      setDrafts(prev => ({ ...prev, [roomId]: content }));
      fetch(`/api/users/${userId}/drafts/${roomId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content }),
      }).catch(() => {});
    }, 500);
  }, []);

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setMessageInput(val);
    if (val) startTyping();
    else stopTyping();
    if (currentUser && currentRoomId) {
      saveDraft(val, currentRoomId, currentUser.id);
    }
  };

  // ── Auto-scroll & mark read ────────────────────────────────────────────────
  useEffect(() => {
    if (!messagesContainerRef.current) return;
    const container = messagesContainerRef.current;
    const isNearBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 100;
    if (isNearBottom) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }

    // Mark last message as read
    if (messages.length > 0 && currentUser && currentRoomId) {
      const lastMsg = messages[messages.length - 1];
      markRead(currentUser.id, currentRoomId, lastMsg.id);
    }
  }, [messages, currentUser, currentRoomId, markRead]);

  const handleScroll = () => {
    if (!messagesContainerRef.current) return;
    const container = messagesContainerRef.current;
    const isNearBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 100;
    setShowScrollBtn(!isNearBottom);
  };

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  // ── Typing text ────────────────────────────────────────────────────────────
  const typingText = (() => {
    const typers = Array.from(typingUsers.values()).filter(name => name !== currentUser?.username);
    if (typers.length === 0) return '';
    if (typers.length === 1) return `${typers[0]} is typing...`;
    if (typers.length === 2) return `${typers[0]} and ${typers[1]} are typing...`;
    return 'Multiple users are typing...';
  })();

  // ── Read receipt helpers ───────────────────────────────────────────────────
  const getReadReceipt = (msgId: number, senderId: number) => {
    const readers = readReceipts[msgId] || [];
    const others = readers.filter(r => r.userId !== currentUser?.id && r.userId !== senderId);
    if (others.length === 0) return null;
    const names = others.map(r => r.username).join(', ');
    return `Seen by ${names}`;
  };

  // ── Reactions ─────────────────────────────────────────────────────────────
  const EMOJI_OPTIONS = ['👍', '❤️', '😂', '😮', '😢'];

  const handleToggleReaction = async (messageId: number, emoji: string) => {
    if (!currentUser) return;
    await fetch(`/api/messages/${messageId}/reactions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ userId: currentUser.id, emoji }),
    });
  };

  const handleStartEdit = (msg: Message) => {
    setEditingMessageId(msg.id);
    setEditInput(msg.content);
  };

  const handleCancelEdit = () => {
    setEditingMessageId(null);
    setEditInput('');
  };

  const handleSubmitEdit = async (messageId: number) => {
    if (!currentUser || !editInput.trim()) return;
    try {
      await fetch(`/api/messages/${messageId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content: editInput.trim() }),
      });
      setEditingMessageId(null);
      setEditInput('');
    } catch {}
  };

  const handleViewEditHistory = async (messageId: number) => {
    try {
      const res = await fetch(`/api/messages/${messageId}/edits`);
      const edits: MessageEdit[] = await res.json();
      setEditHistory(edits);
      setEditHistoryMessageId(messageId);
    } catch {}
  };

  const handleOpenThread = async (msg: Message) => {
    setThreadParentMsg(msg);
    setThreadOpenMessageId(msg.id);
    setThreadReplyInput('');
    try {
      const res = await fetch(`/api/messages/${msg.id}/replies`);
      const replies: Message[] = await res.json();
      setThreadReplies(replies);
    } catch {
      setThreadReplies([]);
    }
  };

  const handleSendReply = async () => {
    if (!currentUser || !threadOpenMessageId || !threadReplyInput.trim()) return;
    const content = threadReplyInput.trim();
    setThreadReplyInput('');
    try {
      await fetch(`/api/messages/${threadOpenMessageId}/replies`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content }),
      });
    } catch {}
  };

  const getReactionGroups = (messageId: number) => {
    const msgReactions = reactions[messageId] || [];
    const groups: Record<string, { count: number; users: string[]; hasMe: boolean }> = {};
    for (const r of msgReactions) {
      if (!groups[r.emoji]) groups[r.emoji] = { count: 0, users: [], hasMe: false };
      groups[r.emoji].count++;
      groups[r.emoji].users.push(r.username);
      if (r.userId === currentUser?.id) groups[r.emoji].hasMe = true;
    }
    return groups;
  };

  // ── Group messages ─────────────────────────────────────────────────────────
  const groupMessages = (msgs: Message[]) => {
    const groups: { author: string; userId: number; time: string; msgs: Message[] }[] = [];
    for (const msg of msgs) {
      const last = groups[groups.length - 1];
      const msgTime = new Date(msg.createdAt);
      if (last && last.userId === msg.userId) {
        last.msgs.push(msg);
      } else {
        groups.push({
          author: msg.username,
          userId: msg.userId,
          time: msgTime.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
          msgs: [msg],
        });
      }
    }
    return groups;
  };

  // ── Presence helpers ───────────────────────────────────────────────────────
  const getStatusColor = (status: string | undefined) => {
    switch (status) {
      case 'online': return 'var(--success)';
      case 'away': return '#f0c040';
      case 'dnd': return 'var(--danger)';
      case 'invisible':
      case 'offline':
      default: return 'var(--text-muted)';
    }
  };

  const getLastActive = (userId: number): string | null => {
    const presence = userPresence[userId];
    if (!presence) return null;
    if (presence.status === 'online') return null;
    const diff = Date.now() - new Date(presence.lastActiveAt).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Last active just now';
    if (mins < 60) return `Last active ${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `Last active ${hours}h ago`;
    return `Last active ${Math.floor(hours / 24)}d ago`;
  };

  // ── Login screen ───────────────────────────────────────────────────────────
  if (!currentUser) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>PostgreSQL Chat</h1>
          <p>Enter a display name to get started</p>
          {loginError && <div className="error-msg">{loginError}</div>}
          <input
            type="text"
            placeholder="Your display name"
            value={loginName}
            onChange={e => { setLoginName(e.target.value); setLoginError(''); }}
            onKeyDown={e => e.key === 'Enter' && handleLogin()}
            maxLength={32}
            autoFocus
          />
          <button onClick={handleLogin}>Join Chat</button>
          <div style={{ margin: '12px 0', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.85rem' }}>or</div>
          <button
            onClick={handleJoinAsGuest}
            style={{ background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-muted)', width: '100%' }}
          >
            Join as Guest
          </button>
          <p style={{ fontSize: '0.75rem', color: 'var(--text-muted)', marginTop: '8px', textAlign: 'center' }}>
            Guest sessions are temporary. You can register later to preserve your history.
          </p>
        </div>
      </div>
    );
  }

  if (!connected) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>PostgreSQL Chat</h1>
          <div className="spinner" style={{ margin: '0 auto 12px' }} />
          <p>Connecting...</p>
        </div>
      </div>
    );
  }

  const currentRoom = rooms.find(r => r.id === currentRoomId);
  const groups = groupMessages(messages);
  const isCurrentUserAdmin = currentRoomId !== null && roomMembers.some(m => m.userId === currentUser?.id && m.role === 'admin');

  return (
    <div className="app-layout">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h2>PostgreSQL Chat</h2>
          <div className="user-info">
            <span className="status-dot" style={{ background: getStatusColor(myStatus) }} />
            <span>{currentUser.username}</span>
            {currentUser.isAnonymous && (
              <span style={{ fontSize: '0.7rem', background: 'var(--warning)', color: '#fff', borderRadius: '4px', padding: '1px 5px', marginLeft: '4px' }}>Guest</span>
            )}
          </div>
          {currentUser.isAnonymous && (
            <button
              onClick={() => { setShowRegisterModal(true); setRegisterError(''); setRegisterName(''); }}
              style={{ marginTop: '6px', width: '100%', padding: '5px', borderRadius: '6px', background: 'var(--primary)', color: '#fff', border: 'none', cursor: 'pointer', fontSize: '0.82rem', fontWeight: 600 }}
            >
              Register Account
            </button>
          )}
          {!currentUser.isAnonymous && (
            <select
              value={myStatus}
              onChange={e => handleStatusChange(e.target.value)}
              style={{ marginTop: '6px', width: '100%', padding: '4px 6px', borderRadius: '6px', background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)', fontSize: '0.8rem' }}
              aria-label="Set your status"
            >
              <option value="online">Online</option>
              <option value="away">Away</option>
              <option value="dnd">Do Not Disturb</option>
              <option value="invisible">Invisible</option>
            </select>
          )}
        </div>

        {/* Registration modal */}
        {showRegisterModal && (
          <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', zIndex: 1000, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
            onClick={e => { if (e.target === e.currentTarget) setShowRegisterModal(false); }}
          >
            <div style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: '10px', padding: '28px', width: '340px', display: 'flex', flexDirection: 'column', gap: '12px' }}>
              <h3 style={{ margin: 0 }}>Register Account</h3>
              <p style={{ margin: 0, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                Your messages, rooms, and history will be preserved.
              </p>
              {registerError && <div className="error-msg">{registerError}</div>}
              <input
                type="text"
                placeholder="Choose a username"
                value={registerName}
                onChange={e => { setRegisterName(e.target.value); setRegisterError(''); }}
                onKeyDown={e => e.key === 'Enter' && handleRegister()}
                maxLength={32}
                autoFocus
                style={{ padding: '8px 12px', borderRadius: '6px', border: '1px solid var(--border)', background: 'var(--bg)', color: 'var(--text)', fontSize: '0.95rem' }}
              />
              <div style={{ display: 'flex', gap: '8px' }}>
                <button onClick={handleRegister} style={{ flex: 1, padding: '8px', borderRadius: '6px', background: 'var(--primary)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600 }}>
                  Register
                </button>
                <button onClick={() => setShowRegisterModal(false)} style={{ flex: 1, padding: '8px', borderRadius: '6px', background: 'transparent', color: 'var(--text-muted)', border: '1px solid var(--border)', cursor: 'pointer' }}>
                  Cancel
                </button>
              </div>
            </div>
          </div>
        )}

        {pendingInvites.length > 0 && (
          <div style={{ padding: '8px', borderBottom: '1px solid var(--border)' }}>
            <div className="sidebar-section-title" style={{ marginBottom: '4px' }}>Room Invitations</div>
            {pendingInvites.map(invite => (
              <div key={invite.inviteId} style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: '6px', padding: '8px', marginBottom: '6px' }}>
                <div style={{ fontSize: '0.82rem', marginBottom: '6px' }}>
                  <strong>{invite.inviterUsername}</strong> invited you to <strong>#{invite.roomName}</strong>
                </div>
                <div style={{ display: 'flex', gap: '6px' }}>
                  <button
                    onClick={() => handleAcceptInvite(invite)}
                    style={{ flex: 1, fontSize: '0.75rem', padding: '4px', background: 'var(--success)', color: '#fff', border: 'none', borderRadius: '4px', cursor: 'pointer' }}
                  >Accept</button>
                  <button
                    onClick={() => handleDeclineInvite(invite)}
                    style={{ flex: 1, fontSize: '0.75rem', padding: '4px', background: 'var(--danger)', color: '#fff', border: 'none', borderRadius: '4px', cursor: 'pointer' }}
                  >Decline</button>
                </div>
              </div>
            ))}
          </div>
        )}

        <div className="sidebar-section">
          <div className="sidebar-section-title">Rooms</div>
        </div>

        <div className="room-list">
          {rooms.length === 0 && (
            <div style={{ padding: '8px', color: 'var(--text-muted)', fontSize: '0.82rem' }}>
              Create a room to get started
            </div>
          )}
          {rooms.map(room => {
            const activity = roomActivity[room.id];
            return (
              <div
                key={room.id}
                className={`room-item ${currentRoomId === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room.id)}
              >
                <span className="room-name">{room.name.startsWith('__dm_') && room.name.endsWith('__') ? (() => { const parts = room.name.slice(5, -2).split('_'); const ids = parts.map(Number); const otherId = ids.find(id => id !== currentUser?.id) ?? ids[0]; const other = onlineUsers.find(u => u.id === otherId); return `@ ${other?.username ?? knownUsers[otherId] ?? `User ${otherId}`}`; })() : `# ${room.name}`}</span>
                <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  {activity?.level === 'hot' && (
                    <span className="activity-badge activity-hot" title={`${activity.recentCount} messages in last 5 min`}>🔥 Hot</span>
                  )}
                  {activity?.level === 'active' && (
                    <span className="activity-badge activity-active" title={`${activity.recentCount} messages in last 5 min`}>Active</span>
                  )}
                  {unreadCounts[room.id] > 0 && currentRoomId !== room.id && (
                    <span className="unread-badge">{unreadCounts[room.id]}</span>
                  )}
                </span>
              </div>
            );
          })}
        </div>

        <div className="create-room-form" style={{ flexWrap: 'wrap' }}>
          <input
            type="text"
            placeholder="New room..."
            value={newRoomName}
            onChange={e => setNewRoomName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
            maxLength={64}
          />
          <label style={{ display: 'flex', alignItems: 'center', gap: '4px', fontSize: '0.8rem', color: 'var(--text-muted)', cursor: 'pointer', whiteSpace: 'nowrap' }}>
            <input
              type="checkbox"
              checked={newRoomIsPrivate}
              onChange={e => setNewRoomIsPrivate(e.target.checked)}
              style={{ cursor: 'pointer' }}
            />
            Private
          </label>
          <button onClick={handleCreateRoom}>+</button>
        </div>

        {currentRoomId !== null && roomMembers.length > 0 && (
          <div className="online-users" style={{ borderTop: '1px solid var(--border)', paddingTop: '8px' }}>
            <div className="sidebar-section-title" style={{ marginBottom: '6px' }}>
              Room Members ({roomMembers.length})
            </div>
            {roomMembers.map(member => (
              <div key={member.userId} className="online-user" style={{ justifyContent: 'space-between', alignItems: 'center' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <span className="status-dot" style={{ background: getStatusColor(userPresence[member.userId]?.status || (onlineUsers.some(u => u.id === member.userId) ? 'online' : 'offline')) }} />
                  <span>{member.username}</span>
                  {member.role === 'admin' && <span style={{ fontSize: '0.7rem', background: 'var(--primary)', color: '#fff', borderRadius: '4px', padding: '1px 5px' }}>admin</span>}
                </div>
                {isCurrentUserAdmin && member.userId !== currentUser?.id && (
                  <div style={{ display: 'flex', gap: '4px' }}>
                    {member.role !== 'admin' && (
                      <button
                        className="cancel-scheduled-btn"
                        style={{ fontSize: '0.7rem', padding: '2px 5px' }}
                        title="Promote to admin"
                        onClick={() => handlePromoteUser(member.userId)}
                      >Promote</button>
                    )}
                    <button
                      className="cancel-scheduled-btn"
                      style={{ fontSize: '0.7rem', padding: '2px 5px' }}
                      title="Kick user"
                      onClick={() => handleKickUser(member.userId)}
                    >Kick</button>
                    <button
                      className="cancel-scheduled-btn"
                      style={{ fontSize: '0.7rem', padding: '2px 5px', background: 'var(--danger)' }}
                      title="Ban user"
                      onClick={() => handleBanUser(member.userId)}
                    >ban</button>
                  </div>
                )}
              </div>
            ))}
            {isCurrentUserAdmin && currentRoom?.isPrivate && (
              <div style={{ marginTop: '8px' }}>
                <div style={{ fontSize: '0.75rem', color: 'var(--text-muted)', marginBottom: '4px' }}>Invite by username</div>
                <div style={{ display: 'flex', gap: '4px' }}>
                  <input
                    type="text"
                    placeholder="Username..."
                    value={inviteUsername}
                    onChange={e => { setInviteUsername(e.target.value); setInviteError(''); }}
                    onKeyDown={e => e.key === 'Enter' && handleInviteUser()}
                    maxLength={32}
                    style={{ flex: 1, fontSize: '0.75rem', padding: '3px 6px', background: 'var(--surface)', color: 'var(--text)', border: '1px solid var(--border)', borderRadius: '4px' }}
                  />
                  <button
                    className="cancel-scheduled-btn"
                    style={{ fontSize: '0.75rem', padding: '3px 7px' }}
                    onClick={handleInviteUser}
                  >Invite</button>
                </div>
                {inviteError && <div style={{ fontSize: '0.7rem', color: 'var(--danger)', marginTop: '2px' }}>{inviteError}</div>}
              </div>
            )}
          </div>
        )}

        <div className="online-users">
          <div className="sidebar-section-title" style={{ marginBottom: '6px' }}>
            Online ({onlineUsers.filter(u => userPresence[u.id]?.status !== 'invisible').length})
          </div>
          {onlineUsers.filter(u => userPresence[u.id]?.status !== 'invisible' || u.id === currentUser?.id).map(u => {
            const presence = userPresence[u.id];
            const status = presence?.status || 'online';
            const lastActive = getLastActive(u.id);
            return (
              <div key={u.id} className="online-user" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '2px' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px', width: '100%' }}>
                  <span className="status-dot" style={{ background: getStatusColor(status) }} title={status} />
                  <span style={{ flex: 1 }}>{u.username}</span>
                  {status === 'dnd' && <span style={{ fontSize: '0.65rem', color: 'var(--danger)' }}>DND</span>}
                  {status === 'away' && <span style={{ fontSize: '0.65rem', color: '#f0c040' }}>Away</span>}
                  {status === 'invisible' && <span style={{ fontSize: '0.65rem', color: 'var(--text-muted)' }}>Invisible</span>}
                  {u.id !== currentUser?.id && (
                    <button
                      className="cancel-scheduled-btn"
                      style={{ fontSize: '0.7rem', padding: '2px 5px' }}
                      title={`DM ${u.username}`}
                      onClick={() => handleStartDM(u.id)}
                    >💬</button>
                  )}
                </div>
                {lastActive && (
                  <span style={{ fontSize: '0.7rem', color: 'var(--text-muted)', paddingLeft: '18px' }}>{lastActive}</span>
                )}
              </div>
            );
          })}
        </div>
      </aside>

      {/* Edit History Modal */}
      {editHistoryMessageId !== null && (
        <div className="modal-backdrop" onClick={() => setEditHistoryMessageId(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-header">
              <span>Edit History</span>
              <button className="close-btn" onClick={() => setEditHistoryMessageId(null)}>✕</button>
            </div>
            <div className="modal-body">
              {editHistory.length === 0 ? (
                <div style={{ color: 'var(--text-muted)' }}>No edit history found.</div>
              ) : (
                editHistory.map(edit => (
                  <div key={edit.id} className="edit-history-item">
                    <div className="edit-history-content">{edit.previousContent}</div>
                    <div className="edit-history-meta">
                      Edited by {edit.username} at {new Date(edit.editedAt).toLocaleString()}
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* Thread panel */}
      {threadOpenMessageId !== null && threadParentMsg && (
        <div className="thread-panel">
          <div className="thread-panel-header">
            <span>Thread</span>
            <button className="close-btn" onClick={() => setThreadOpenMessageId(null)}>✕</button>
          </div>
          <div className="thread-panel-body">
            <div className="thread-parent-msg">
              <div className="thread-parent-author">{threadParentMsg.username}</div>
              <div className="thread-parent-content">{threadParentMsg.content}</div>
            </div>
            <div className="thread-replies-divider">{threadReplies.length} {threadReplies.length === 1 ? 'reply' : 'replies'}</div>
            <div className="thread-replies-list">
              {threadReplies.map(reply => (
                <div key={reply.id} className="thread-reply-item">
                  <div className="thread-reply-author">{reply.username}</div>
                  <div className="thread-reply-content">{reply.content}</div>
                  <div className="thread-reply-time">{new Date(reply.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</div>
                </div>
              ))}
            </div>
          </div>
          <div className="thread-reply-input">
            <input
              type="text"
              placeholder="Reply..."
              value={threadReplyInput}
              onChange={e => setThreadReplyInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSendReply()}
              maxLength={2000}
              autoFocus
            />
            <button onClick={handleSendReply} disabled={!threadReplyInput.trim()}>Reply</button>
          </div>
        </div>
      )}

      {/* Main area */}
      <main className="main-area">
        {!currentRoom ? (
          <div className="no-room">
            {kickedNotice
              ? <><div className="error-msg" style={{ margin: '0 auto', maxWidth: '400px' }}>{kickedNotice}</div><p style={{ color: 'var(--text-muted)', marginTop: '8px' }}>Select another room to continue.</p></>
              : <p>Select or create a room to start chatting</p>
            }
          </div>
        ) : (
          <>
            <div className="room-header">
              <h3>{currentRoom.name.startsWith('__dm_') && currentRoom.name.endsWith('__') ? (() => { const parts = currentRoom.name.slice(5, -2).split('_'); const ids = parts.map(Number); const otherId = ids.find(id => id !== currentUser?.id) ?? ids[0]; const other = onlineUsers.find(u => u.id === otherId); return `@ ${other?.username ?? knownUsers[otherId] ?? `User ${otherId}`}`; })() : `# ${currentRoom.name}`}</h3>
              {isCurrentUserAdmin && (
                <span style={{ fontSize: '0.75rem', background: 'var(--primary)', color: '#fff', borderRadius: '4px', padding: '2px 8px' }}>Admin</span>
              )}
              <button className="leave-btn" onClick={handleLeaveRoom}>Leave</button>
            </div>

            <div className="messages-wrapper">
              <div
                className="messages-container"
                ref={messagesContainerRef}
                onScroll={handleScroll}
              >
                {messages.length === 0 && (
                  <div style={{ color: 'var(--text-muted)', textAlign: 'center', marginTop: '40px' }}>
                    No messages yet. Say hello!
                  </div>
                )}
                {groups.map((group, gi) => (
                  <div key={`${group.userId}-${gi}`} className="message-group">
                    <div className="message-header">
                      <span className="message-author">{group.author}</span>
                      <span className="message-time">{group.time}</span>
                    </div>
                    {group.msgs.map(msg => {
                      const receipt = getReadReceipt(msg.id, group.userId);
                      const reactionGroups = getReactionGroups(msg.id);
                      const hasReactions = Object.keys(reactionGroups).length > 0;
                      return (
                        <div
                          key={msg.id}
                          className={`message-item${msg.expiresAt ? ' ephemeral-message' : ''}`}
                          onMouseEnter={() => setHoveredMessage(msg.id)}
                          onMouseLeave={() => setHoveredMessage(null)}
                        >
                          {editingMessageId === msg.id ? (
                            <div className="edit-form">
                              <input
                                type="text"
                                value={editInput}
                                onChange={e => setEditInput(e.target.value)}
                                onKeyDown={e => {
                                  if (e.key === 'Enter') handleSubmitEdit(msg.id);
                                  if (e.key === 'Escape') handleCancelEdit();
                                }}
                                autoFocus
                                maxLength={2000}
                              />
                              <button onClick={() => handleSubmitEdit(msg.id)} disabled={!editInput.trim()}>Save</button>
                              <button onClick={handleCancelEdit} className="cancel-edit-btn">Cancel</button>
                            </div>
                          ) : (
                            <div className="message-content">
                              {msg.content}
                              {msg.editedAt && (
                                <span
                                  className="edited-indicator"
                                  onClick={() => handleViewEditHistory(msg.id)}
                                  title="Click to view edit history"
                                > (edited)</span>
                              )}
                            </div>
                          )}
                          {msg.expiresAt && (
                            <div className="ephemeral-indicator">⏳ {getEphemeralCountdown(msg.expiresAt)}</div>
                          )}
                          {hasReactions && (
                            <div className="reaction-list">
                              {Object.entries(reactionGroups).map(([emoji, info]) => (
                                <button
                                  key={emoji}
                                  className={`reaction-btn${info.hasMe ? ' reacted' : ''}`}
                                  onClick={() => handleToggleReaction(msg.id, emoji)}
                                  title={info.users.join(', ')}
                                >
                                  {emoji} {info.count}
                                </button>
                              ))}
                            </div>
                          )}
                          {hoveredMessage === msg.id && editingMessageId !== msg.id && (
                            <div className="emoji-picker">
                              {EMOJI_OPTIONS.map(emoji => (
                                <button
                                  key={emoji}
                                  className="emoji-option"
                                  onClick={() => handleToggleReaction(msg.id, emoji)}
                                  title={`React with ${emoji}`}
                                >
                                  {emoji}
                                </button>
                              ))}
                              <button
                                className="emoji-option"
                                onClick={() => handleOpenThread(msg)}
                                title="Reply in thread"
                              >
                                💬
                              </button>
                              {msg.userId === currentUser?.id && (
                                <button
                                  className="emoji-option"
                                  onClick={() => handleStartEdit(msg)}
                                  title="Edit message"
                                >
                                  ✏️
                                </button>
                              )}
                            </div>
                          )}
                          {(msg.replyCount ?? 0) > 0 && (
                            <button
                              className="reply-count-btn"
                              onClick={() => handleOpenThread(msg)}
                            >
                              💬 {msg.replyCount} {msg.replyCount === 1 ? 'reply' : 'replies'}
                            </button>
                          )}
                          {receipt && <div className="read-receipt">{receipt}</div>}
                        </div>
                      );
                    })}
                  </div>
                ))}
                <div ref={messagesEndRef} />
              </div>

              {showScrollBtn && (
                <button className="scroll-to-bottom" onClick={scrollToBottom} title="Scroll to bottom">
                  ↓
                </button>
              )}
            </div>

            <div className="typing-indicator">{typingText}</div>

            {showSchedulePanel && (
              <div className="schedule-panel">
                <div className="schedule-panel-header">
                  <span>Schedule Message</span>
                  <button className="close-btn" onClick={() => { setShowSchedulePanel(false); setScheduleError(''); }}>✕</button>
                </div>
                {scheduleError && <div className="error-msg">{scheduleError}</div>}
                <input
                  type="text"
                  placeholder="Message content..."
                  value={scheduleInput}
                  onChange={e => setScheduleInput(e.target.value)}
                  maxLength={2000}
                />
                <input
                  type="datetime-local"
                  value={scheduleTime}
                  onChange={e => setScheduleTime(e.target.value)}
                  min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}
                />
                <button onClick={handleScheduleMessage} disabled={!scheduleInput.trim() || !scheduleTime}>
                  Schedule
                </button>
              </div>
            )}

            {scheduledMessages.filter(m => m.roomId === currentRoomId).length > 0 && (
              <div className="scheduled-messages-list">
                <div className="scheduled-messages-title">Scheduled (this room)</div>
                {scheduledMessages.filter(m => m.roomId === currentRoomId).map(sm => (
                  <div key={sm.id} className="scheduled-message-item">
                    <div className="scheduled-message-content">{sm.content}</div>
                    <div className="scheduled-message-meta">
                      Sends at {new Date(sm.scheduledAt).toLocaleString()}
                    </div>
                    <button className="cancel-scheduled-btn" onClick={() => handleCancelScheduled(sm.id)}>Cancel</button>
                  </div>
                ))}
              </div>
            )}

            {currentRoomId && drafts[currentRoomId] && drafts[currentRoomId] !== messageInput && (
              <div style={{ fontSize: '11px', color: '#848484', padding: '2px 8px' }}>Draft saved</div>
            )}
            <div className="input-bar">
              <input
                type="text"
                placeholder={`Message #${currentRoom.name}${ephemeralSeconds ? ` (disappears in ${ephemeralSeconds >= 60 ? ephemeralSeconds / 60 + 'm' : ephemeralSeconds + 's'})` : ''}${currentRoomId && drafts[currentRoomId] ? ' (draft)' : ''}`}
                value={messageInput}
                onChange={handleInputChange}
                onKeyDown={handleKeyDown}
                maxLength={2000}
                autoFocus
              />
              <button onClick={handleSend} disabled={!messageInput.trim()}>Send</button>
              <select
                className="ephemeral-select"
                value={ephemeralSeconds ?? ''}
                onChange={e => setEphemeralSeconds(e.target.value ? parseInt(e.target.value) : null)}
                title="Send as ephemeral message"
              >
                <option value="">Normal</option>
                <option value="60">1 min</option>
                <option value="300">5 min</option>
                <option value="600">10 min</option>
                <option value="3600">1 hour</option>
              </select>
              <button
                className="schedule-btn"
                onClick={() => { setShowSchedulePanel(p => !p); setScheduleError(''); }}
                title="Schedule a message"
              >
                ⏰
              </button>
            </div>
          </>
        )}
      </main>
    </div>
  );
}

export default App;
