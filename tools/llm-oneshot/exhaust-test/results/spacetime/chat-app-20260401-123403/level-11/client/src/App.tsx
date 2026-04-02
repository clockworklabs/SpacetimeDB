import { useState, useEffect, useRef, useCallback } from 'react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';

function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Subscription state
  const [subscribed, setSubscribed] = useState(false);

  // UI state
  const [nameInput, setNameInput] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [newRoomIsPrivate, setNewRoomIsPrivate] = useState(false);
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [error, setError] = useState('');

  // Private room / invite UI state
  const [showInvitePanel, setShowInvitePanel] = useState(false);
  const [inviteIdentityInput, setInviteIdentityInput] = useState('');

  // Scheduled message UI state
  const [showScheduler, setShowScheduler] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');

  // Edit message UI state
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editInput, setEditInput] = useState('');
  const [showHistoryFor, setShowHistoryFor] = useState<bigint | null>(null);

  // Permissions UI state
  const [showMemberManager, setShowMemberManager] = useState(false);

  // Presence UI state
  const [showStatusMenu, setShowStatusMenu] = useState(false);

  // Threading UI state
  const [openThreadMessageId, setOpenThreadMessageId] = useState<bigint | null>(null);
  const [threadReplyInput, setThreadReplyInput] = useState('');

  // Ephemeral message UI state (0 = normal, 60 = 1 min, 300 = 5 min)
  const [ephemeralDuration, setEphemeralDuration] = useState<number>(0);
  // Tick counter to re-render countdowns every second
  const [, setTick] = useState(0);

  // Typing debounce refs
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastTypingSentRef = useRef(0);

  // Messages end ref for auto-scroll
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Save token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe when connected
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM typing_state',
        'SELECT * FROM user_room_state',
        'SELECT * FROM scheduled_message',
        'SELECT * FROM reaction',
        'SELECT * FROM message_edit',
        'SELECT * FROM room_permission',
        'SELECT * FROM thread_reply',
        'SELECT * FROM room_invitation',
        'SELECT * FROM room_activity',
        'SELECT * FROM message_draft',
      ]);
  }, [conn, isActive]);

  // Table data
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingStates] = useTable(tables.typingState);
  const [userRoomStates] = useTable(tables.userRoomState);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [reactions] = useTable(tables.reaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [roomPermissions] = useTable(tables.roomPermission);
  const [threadReplies] = useTable(tables.threadReply);
  const [roomInvitations] = useTable(tables.roomInvitation);
  const [roomActivities] = useTable(tables.roomActivity);
  const [messageDrafts] = useTable(tables.messageDraft);

  // Draft save debounce ref
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Track the last draft text we set from server (to avoid overwriting user's active typing)
  const lastServerDraftRef = useRef<string>('');

  // Derived: my user record
  const myUser = myIdentity
    ? users.find(u => u.identity.toHexString() === myIdentity.toHexString())
    : undefined;

  // Derived: am I admin in selected room?
  const amIAdmin = selectedRoomId !== null && myIdentity
    ? roomPermissions.some(
        p => p.roomId === selectedRoomId && p.userIdentity.toHexString() === myIdentity.toHexString() && p.role === 'admin'
      )
    : false;

  // Derived: am I banned from selected room?
  const amIBanned = selectedRoomId !== null && myIdentity
    ? roomPermissions.some(
        p => p.roomId === selectedRoomId && p.userIdentity.toHexString() === myIdentity.toHexString() && p.role === 'banned'
      )
    : false;

  // Derived: rooms I'm a member of
  const myMemberships = myIdentity
    ? roomMembers.filter(m => m.userIdentity.toHexString() === myIdentity.toHexString())
    : [];
  const joinedRoomIds = new Set(myMemberships.map(m => m.roomId));

  // Derived: messages in selected room, sorted
  const roomMessages = selectedRoomId !== null
    ? messages
        .filter(m => m.roomId === selectedRoomId)
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
    : [];

  // Derived: last message id in selected room
  const latestMessageId = roomMessages.length > 0 ? roomMessages[roomMessages.length - 1].id : null;

  // Derived: unread count per room
  const getUnreadCount = useCallback((roomId: bigint): number => {
    if (!myIdentity) return 0;
    const state = userRoomStates.find(
      s => s.roomId === roomId && s.userIdentity.toHexString() === myIdentity.toHexString()
    );
    const lastRead = state?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastRead).length;
  }, [myIdentity, userRoomStates, messages]);

  // Derived: typing users in current room (excluding self, excluding expired)
  const now = BigInt(Date.now()) * 1000n;
  const typingUsersInRoom = selectedRoomId !== null
    ? typingStates
        .filter(ts =>
          ts.roomId === selectedRoomId &&
          ts.expiresAtMicros > now &&
          ts.userIdentity.toHexString() !== myIdentity?.toHexString()
        )
        .map(ts => users.find(u => u.identity.toHexString() === ts.userIdentity.toHexString()))
        .filter((u): u is NonNullable<typeof u> => u !== undefined)
    : [];

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [roomMessages.length]);

  // Mark messages as read when room opens or new messages arrive
  useEffect(() => {
    if (!conn || !selectedRoomId || !latestMessageId || !myUser) return;
    const state = userRoomStates.find(
      s => s.roomId === selectedRoomId && s.userIdentity.toHexString() === myIdentity?.toHexString()
    );
    if (!state || latestMessageId > state.lastReadMessageId) {
      conn.reducers.markRead({ roomId: selectedRoomId, messageId: latestMessageId });
    }
  }, [conn, selectedRoomId, latestMessageId, myUser, myIdentity, userRoomStates]);

  // Clear typing when switching rooms
  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    };
  }, [selectedRoomId]);

  // Deselect room if kicked/banned (membership removed)
  useEffect(() => {
    if (selectedRoomId === null || !myIdentity) return;
    const stillMember = roomMembers.some(
      m => m.roomId === selectedRoomId && m.userIdentity.toHexString() === myIdentity.toHexString()
    );
    if (!stillMember) {
      setSelectedRoomId(null);
      setShowMemberManager(false);
    }
  }, [roomMembers, selectedRoomId, myIdentity]);

  // Tick every second to keep ephemeral countdowns current
  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  // Live draft sync: update messageInput when the server-side draft changes
  // (handles cross-session sync — another tab saves a draft, this tab sees it)
  useEffect(() => {
    if (!selectedRoomId || !myIdentity) return;
    const draft = messageDrafts.find(
      d => d.roomId === selectedRoomId && d.userIdentity.toHexString() === myIdentity.toHexString()
    );
    const serverText = draft?.text ?? '';
    // Only update if the server draft changed (not just a round-trip echo of our own save)
    if (serverText !== lastServerDraftRef.current) {
      lastServerDraftRef.current = serverText;
      setMessageInput(serverText);
    }
  }, [messageDrafts, selectedRoomId, myIdentity]);

  // Auto-away: set status to 'away' after 5 minutes of inactivity
  const autoAwayTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isAutoAwayRef = useRef(false);
  useEffect(() => {
    if (!conn || !myUser) return;
    const resetTimer = () => {
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
      // If we auto-set away and user is now active, restore to online
      if (isAutoAwayRef.current && myUser.status === 'away') {
        conn.reducers.setStatus({ status: 'online' });
        isAutoAwayRef.current = false;
      }
      autoAwayTimerRef.current = setTimeout(() => {
        if (myUser.status === 'online') {
          conn.reducers.setStatus({ status: 'away' });
          isAutoAwayRef.current = true;
        }
      }, 5 * 60 * 1000); // 5 minutes
    };
    const events = ['mousemove', 'keydown', 'mousedown', 'touchstart'];
    events.forEach(e => document.addEventListener(e, resetTimer));
    resetTimer();
    return () => {
      events.forEach(e => document.removeEventListener(e, resetTimer));
      if (autoAwayTimerRef.current) clearTimeout(autoAwayTimerRef.current);
    };
  }, [conn, myUser]);

  // ── Actions ────────────────────────────────────────────────────────────

  const handleRegister = () => {
    if (!conn || !nameInput.trim()) return;
    setError('');
    conn.reducers.register({ name: nameInput.trim() });
    setNameInput('');
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    setError('');
    conn.reducers.createRoom({ name: newRoomName.trim(), isPrivate: newRoomIsPrivate });
    setNewRoomName('');
    setNewRoomIsPrivate(false);
    setShowCreateRoom(false);
  };

  const handleInviteUser = () => {
    if (!conn || !selectedRoomId || !inviteIdentityInput.trim()) return;
    try {
      conn.reducers.inviteToRoom({ roomId: selectedRoomId, inviteeIdentity: Identity.fromString(inviteIdentityInput.trim()) });
      setInviteIdentityInput('');
      setShowInvitePanel(false);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleAcceptInvitation = (invitationId: bigint) => {
    if (!conn) return;
    conn.reducers.acceptInvitation({ invitationId });
  };

  const handleDeclineInvitation = (invitationId: bigint) => {
    if (!conn) return;
    conn.reducers.declineInvitation({ invitationId });
  };

  const handleOpenDm = (targetIdentityHex: string) => {
    if (!conn) return;
    conn.reducers.openDm({ targetIdentity: Identity.fromString(targetIdentityHex) });
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  };

  const handleSelectRoom = (roomId: bigint) => {
    // Save current draft for the room we're leaving
    if (conn && selectedRoomId !== null) {
      if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
      conn.reducers.saveDraft({ roomId: selectedRoomId, text: messageInput });
    }
    // Load draft for the new room synchronously from cached table data
    const draft = myIdentity
      ? messageDrafts.find(d => d.roomId === roomId && d.userIdentity.toHexString() === myIdentity.toHexString())
      : undefined;
    const draftText = draft?.text ?? '';
    lastServerDraftRef.current = draftText;
    setMessageInput(draftText);
    setSelectedRoomId(roomId);
  };

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageInput.trim(), ttlSecs: BigInt(ephemeralDuration) });
    // Clear draft
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    conn.reducers.saveDraft({ roomId: selectedRoomId, text: '' });
    lastServerDraftRef.current = '';
    setMessageInput('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
    lastTypingSentRef.current = 0;
  };

  const handleMessageInput = (text: string) => {
    setMessageInput(text);
    if (!conn || !selectedRoomId) return;
    // Debounced draft save (300ms)
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    draftSaveTimerRef.current = setTimeout(() => {
      conn.reducers.saveDraft({ roomId: selectedRoomId, text });
    }, 300);
    const now2 = Date.now();
    if (now2 - lastTypingSentRef.current > 1500) {
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: true });
      lastTypingSentRef.current = now2;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.setTyping({ roomId: selectedRoomId, isTyping: false });
      lastTypingSentRef.current = 0;
    }, 2000);
  };

  // Seen by: users whose lastReadMessageId >= this message's id
  const getSeenBy = (messageId: bigint) => {
    if (!myIdentity) return [];
    return userRoomStates
      .filter(s => s.roomId === selectedRoomId && s.lastReadMessageId >= messageId)
      .map(s => users.find(u => u.identity.toHexString() === s.userIdentity.toHexString()))
      .filter((u): u is NonNullable<typeof u> => u !== undefined);
  };

  const handleScheduleMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim() || !scheduleTime) return;
    const sendAtMs = new Date(scheduleTime).getTime();
    if (isNaN(sendAtMs) || sendAtMs <= Date.now()) {
      setError('Scheduled time must be in the future');
      return;
    }
    const sendAtMicros = BigInt(sendAtMs) * 1000n;
    conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageInput.trim(), sendAtMicros });
    setMessageInput('');
    setScheduleTime('');
    setShowScheduler(false);
  };

  const handleToggleReaction = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    conn.reducers.toggleReaction({ messageId, emoji });
  };

  const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

  // Get grouped reaction counts for a message: { emoji -> [userName, ...] }
  const getReactionsForMessage = (messageId: bigint): Map<string, string[]> => {
    const map = new Map<string, string[]>();
    for (const r of reactions.filter(r => r.messageId === messageId)) {
      const u = users.find(u => u.identity.toHexString() === r.userIdentity.toHexString());
      const name = u?.name ?? 'Unknown';
      if (!map.has(r.emoji)) map.set(r.emoji, []);
      map.get(r.emoji)!.push(name);
    }
    return map;
  };

  const handleStartEdit = (messageId: bigint, currentText: string) => {
    setEditingMessageId(messageId);
    setEditInput(currentText);
    setShowHistoryFor(null);
  };

  const handleSubmitEdit = (messageId: bigint) => {
    if (!conn || !editInput.trim()) return;
    conn.reducers.editMessage({ messageId, newText: editInput.trim() });
    setEditingMessageId(null);
    setEditInput('');
  };

  const handleCancelEdit = () => {
    setEditingMessageId(null);
    setEditInput('');
  };

  const getEditHistory = (messageId: bigint) =>
    messageEdits
      .filter(e => e.messageId === messageId)
      .sort((a, b) => (a.editedAt.microsSinceUnixEpoch < b.editedAt.microsSinceUnixEpoch ? -1 : 1));

  const handleKickUser = (targetIdentityHex: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.kickUser({ roomId: selectedRoomId, targetIdentity: Identity.fromString(targetIdentityHex) });
  };

  const handleBanUser = (targetIdentityHex: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.banUser({ roomId: selectedRoomId, targetIdentity: Identity.fromString(targetIdentityHex) });
  };

  const handlePromoteAdmin = (targetIdentityHex: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.promoteAdmin({ roomId: selectedRoomId, targetIdentity: Identity.fromString(targetIdentityHex) });
  };

  const handleCancelScheduled = (scheduledId: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  };

  const formatScheduledTime = (scheduledAt: { tag: string; value: { microsSinceUnixEpoch?: bigint } }) => {
    if (scheduledAt.tag === 'Time' && scheduledAt.value.microsSinceUnixEpoch !== undefined) {
      const d = new Date(Number(scheduledAt.value.microsSinceUnixEpoch / 1000n));
      return d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    }
    return 'unknown time';
  };

  const formatTime = (ts: { microsSinceUnixEpoch: bigint }) => {
    const d = new Date(Number(ts.microsSinceUnixEpoch / 1000n));
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Pending scheduled messages in current room from the current user
  const myPendingScheduled = selectedRoomId !== null && myIdentity
    ? scheduledMessages.filter(
        sm => sm.roomId === selectedRoomId && sm.sender.toHexString() === myIdentity.toHexString()
      ).sort((a, b) => {
        const aT = a.scheduledAt.tag === 'Time' ? (a.scheduledAt.value as any).microsSinceUnixEpoch ?? 0n : 0n;
        const bT = b.scheduledAt.tag === 'Time' ? (b.scheduledAt.value as any).microsSinceUnixEpoch ?? 0n : 0n;
        return aT < bT ? -1 : 1;
      })
    : [];

  // Default datetime-local value = now + 5 minutes
  const defaultScheduleTime = () => {
    const d = new Date(Date.now() + 5 * 60 * 1000);
    // format as YYYY-MM-DDTHH:MM (datetime-local format)
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  };

  // Returns "⏱ Xs" or "⏱ Xm Ys" countdown for an ephemeral message, or null if permanent/already expired
  const getEphemeralCountdown = (expiresAtMicros: bigint): string | null => {
    if (expiresAtMicros === 0n) return null;
    const nowMs = Date.now();
    const expiresMs = Number(expiresAtMicros / 1000n);
    const secsLeft = Math.max(0, Math.ceil((expiresMs - nowMs) / 1000));
    if (secsLeft === 0) return '⏱ expiring...';
    if (secsLeft < 60) return `⏱ ${secsLeft}s`;
    const m = Math.floor(secsLeft / 60);
    const s = secsLeft % 60;
    return s > 0 ? `⏱ ${m}m ${s}s` : `⏱ ${m}m`;
  };

  // ── Threading helpers ─────────────────────────────────────────────────

  const getRepliesForMessage = (parentMessageId: bigint) =>
    threadReplies
      .filter(r => r.parentMessageId === parentMessageId)
      .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

  const getReplyCount = (parentMessageId: bigint) =>
    threadReplies.filter(r => r.parentMessageId === parentMessageId).length;

  const handleSendThreadReply = () => {
    if (!conn || !openThreadMessageId || !threadReplyInput.trim()) return;
    conn.reducers.sendThreadReply({ parentMessageId: openThreadMessageId, text: threadReplyInput.trim() });
    setThreadReplyInput('');
  };

  // ── Presence helpers ──────────────────────────────────────────────────

  const handleSetStatus = (status: string) => {
    if (!conn) return;
    conn.reducers.setStatus({ status });
    setShowStatusMenu(false);
  };

  // Returns CSS class for status dot
  const statusDotClass = (user: { online: boolean; status: string }) => {
    if (!user.online) return 'status-dot status-offline';
    switch (user.status) {
      case 'away': return 'status-dot status-away';
      case 'dnd': return 'status-dot status-dnd';
      case 'invisible': return 'status-dot status-invisible';
      default: return 'status-dot status-online';
    }
  };

  // Returns label for status
  const statusLabel = (status: string) => {
    switch (status) {
      case 'away': return 'Away';
      case 'dnd': return 'Do Not Disturb';
      case 'invisible': return 'Invisible';
      default: return 'Online';
    }
  };

  // Returns "Last active X ago" string
  const lastActiveText = (lastActiveAt: { microsSinceUnixEpoch: bigint }) => {
    const nowMs = Date.now();
    const lastMs = Number(lastActiveAt.microsSinceUnixEpoch / 1000n);
    const diffSecs = Math.max(0, Math.floor((nowMs - lastMs) / 1000));
    if (diffSecs < 60) return 'Last active just now';
    if (diffSecs < 3600) return `Last active ${Math.floor(diffSecs / 60)}m ago`;
    if (diffSecs < 86400) return `Last active ${Math.floor(diffSecs / 3600)}h ago`;
    return `Last active ${Math.floor(diffSecs / 86400)}d ago`;
  };

  // ── Render ────────────────────────────────────────────────────────────

  if (!subscribed) {
    return (
      <div className="loading">
        <div className="spinner" />
        <span>Connecting to SpacetimeDB...</span>
      </div>
    );
  }

  // Registration screen
  if (!myUser) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <div className="logo">⚡ SpacetimeDB Chat</div>
          <h2>Set your display name</h2>
          {error && <div className="error">{error}</div>}
          <input
            className="input"
            type="text"
            placeholder="Enter your name..."
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            maxLength={32}
            autoFocus
          />
          <button className="btn btn-primary" onClick={handleRegister} disabled={!nameInput.trim()}>
            Join Chat
          </button>
        </div>
      </div>
    );
  }

  // Pending invitations for current user
  const myPendingInvitations = myIdentity
    ? roomInvitations.filter(
        inv => inv.inviteeIdentity.toHexString() === myIdentity.toHexString() && inv.status === 'pending'
      )
    : [];

  // Sorted rooms: public rooms + private rooms where user is member
  const allRooms = [...rooms]
    .filter(r => !r.isPrivate || joinedRoomIds.has(r.id))
    .sort((a, b) => {
      const aJoined = joinedRoomIds.has(a.id);
      const bJoined = joinedRoomIds.has(b.id);
      if (aJoined && !bJoined) return -1;
      if (!aJoined && bJoined) return 1;
      return a.id < b.id ? -1 : 1;
    });

  const selectedRoom = selectedRoomId !== null ? rooms.find(r => r.id === selectedRoomId) : undefined;
  const onlineUsers = users.filter(u => u.online);

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <span className="sidebar-title">⚡ SpacetimeDB Chat</span>
          <div className="user-badge-wrapper">
            <button
              className="user-badge user-badge-btn"
              title={`Status: ${statusLabel(myUser.status || 'online')}`}
              onClick={() => setShowStatusMenu(s => !s)}
            >
              <span className={statusDotClass(myUser)} />
              {myUser.name}
            </button>
            {showStatusMenu && (
              <div className="status-menu">
                {(['online', 'away', 'dnd', 'invisible'] as const).map(s => (
                  <button key={s} className="status-menu-item" onClick={() => handleSetStatus(s)}>
                    <span className={`status-dot status-${s}`} />
                    {statusLabel(s)}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Rooms section */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="btn-icon" onClick={() => setShowCreateRoom(!showCreateRoom)} title="Create room">+</button>
          </div>

          {showCreateRoom && (
            <div className="create-room-form">
              <input
                className="input input-sm"
                type="text"
                placeholder="Room name..."
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                maxLength={64}
                autoFocus
              />
              <label className="private-room-toggle">
                <input
                  type="checkbox"
                  checked={newRoomIsPrivate}
                  onChange={e => setNewRoomIsPrivate(e.target.checked)}
                />
                <span>Private room</span>
              </label>
              <div className="create-room-actions">
                <button className="btn btn-sm btn-primary" onClick={handleCreateRoom} disabled={!newRoomName.trim()}>
                  Create
                </button>
                <button className="btn btn-sm" onClick={() => { setShowCreateRoom(false); setNewRoomName(''); setNewRoomIsPrivate(false); }}>
                  Cancel
                </button>
              </div>
            </div>
          )}

          <div className="room-list">
            {allRooms.map(room => {
              const isJoined = joinedRoomIds.has(room.id);
              const unread = isJoined ? getUnreadCount(room.id) : 0;
              const isSelected = selectedRoomId === room.id;
              const activity = roomActivities.find(a => a.roomId === room.id);
              const activityLevel = activity?.activityLevel ?? '';
              return (
                <div
                  key={String(room.id)}
                  className={`room-item ${isSelected ? 'room-item-selected' : ''} ${!isJoined ? 'room-item-unjoined' : ''}`}
                  onClick={() => isJoined && handleSelectRoom(room.id)}
                >
                  <span className="room-icon">{room.isDm ? '💬' : room.isPrivate ? '🔒' : '#'}</span>
                  <span className="room-name">{room.name}</span>
                  {room.isPrivate && !room.isDm && <span className="private-badge">private</span>}
                  {activityLevel === 'hot' && <span className="activity-badge activity-hot" title={`${activity?.recentMessageCount ?? 0} messages in last 2 min`}>🔥 Hot</span>}
                  {activityLevel === 'active' && <span className="activity-badge activity-active" title={`${activity?.recentMessageCount ?? 0} messages in last 5 min`}>⚡ Active</span>}
                  <div className="room-actions">
                    {unread > 0 && <span className="unread-badge">{unread}</span>}
                    {isJoined ? (
                      <button
                        className="btn-icon btn-icon-sm btn-danger"
                        onClick={e => { e.stopPropagation(); handleLeaveRoom(room.id); }}
                        title="Leave room"
                      >
                        ×
                      </button>
                    ) : (
                      <button
                        className="btn-icon btn-icon-sm btn-success"
                        onClick={e => { e.stopPropagation(); handleJoinRoom(room.id); }}
                        title="Join room"
                      >
                        +
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
            {allRooms.length === 0 && (
              <div className="empty-rooms">No rooms yet. Create one!</div>
            )}
          </div>
        </div>

        {/* Pending invitations section */}
        {myPendingInvitations.length > 0 && (
          <div className="sidebar-section">
            <div className="sidebar-section-header">
              <span>Invitations <span className="invite-count-badge">{myPendingInvitations.length}</span></span>
            </div>
            <div className="invitation-list">
              {myPendingInvitations.map(inv => {
                const invRoom = rooms.find(r => r.id === inv.roomId);
                const inviter = users.find(u => u.identity.toHexString() === inv.inviterIdentity.toHexString());
                return (
                  <div key={String(inv.id)} className="invitation-item">
                    <div className="invitation-info">
                      <span className="invitation-room">{invRoom?.name ?? 'Unknown room'}</span>
                      <span className="invitation-from">from {inviter?.name ?? 'Unknown'}</span>
                    </div>
                    <div className="invitation-actions">
                      <button className="btn btn-sm btn-success" onClick={() => handleAcceptInvitation(inv.id)}>Accept</button>
                      <button className="btn btn-sm btn-danger" onClick={() => handleDeclineInvitation(inv.id)}>Decline</button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Users section — all users with presence */}
        <div className="sidebar-section sidebar-section-users">
          <div className="sidebar-section-header">
            <span>Users ({onlineUsers.length} online)</span>
          </div>
          <div className="user-list">
            {[...users].sort((a, b) => {
              // Online first, then alphabetical
              if (a.online && !b.online) return -1;
              if (!a.online && b.online) return 1;
              return a.name.localeCompare(b.name);
            }).map(u => {
              const isMe = u.identity.toHexString() === myIdentity?.toHexString();
              return (
                <div key={u.identity.toHexString()} className="user-item">
                  <span className={statusDotClass(u)} />
                  <div className="user-item-info">
                    <span className={isMe ? 'user-name user-name-me' : 'user-name'}>
                      {u.name}{isMe ? ' (you)' : ''}
                    </span>
                    {!u.online && u.lastActiveAt && (
                      <span className="user-last-active">{lastActiveText(u.lastActiveAt)}</span>
                    )}
                    {u.online && u.status && u.status !== 'online' && (
                      <span className="user-status-label">{statusLabel(u.status)}</span>
                    )}
                  </div>
                  {!isMe && (
                    <button
                      className="btn-icon btn-icon-sm dm-btn"
                      onClick={() => handleOpenDm(u.identity.toHexString())}
                      title={`DM ${u.name}`}
                    >
                      💬
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Main chat area */}
      <div className={`main${openThreadMessageId ? ' main-with-thread' : ''}`}>
        {selectedRoom ? (
          <>
            {/* Chat header */}
            <div className="chat-header">
              <span className="chat-title"># {selectedRoom.name}</span>
              <span className="chat-members">
                {roomMembers.filter(m => m.roomId === selectedRoom.id).length} members
              </span>
              {amIAdmin && (
                <button
                  className="btn btn-sm manage-btn"
                  onClick={() => setShowMemberManager(m => !m)}
                  title="Manage members"
                >
                  ⚙ Manage
                </button>
              )}
              {amIAdmin && selectedRoom.isPrivate && (
                <button
                  className="btn btn-sm invite-btn"
                  onClick={() => setShowInvitePanel(p => !p)}
                  title="Invite user"
                >
                  + Invite
                </button>
              )}
            </div>

            {/* Member manager panel (admin only) */}
            {amIAdmin && showMemberManager && (
              <div className="member-manager">
                <div className="member-manager-title">Room Members</div>
                {roomMembers
                  .filter(m => m.roomId === selectedRoom.id)
                  .map(m => {
                    const u = users.find(u => u.identity.toHexString() === m.userIdentity.toHexString());
                    const isMe = m.userIdentity.toHexString() === myIdentity?.toHexString();
                    const isAdminMember = roomPermissions.some(
                      p => p.roomId === selectedRoom.id && p.userIdentity.toHexString() === m.userIdentity.toHexString() && p.role === 'admin'
                    );
                    return (
                      <div key={m.userIdentity.toHexString()} className="member-row">
                        <span className="member-name">
                          {u?.name ?? 'Unknown'}
                          {isMe && ' (you)'}
                          {isAdminMember && <span className="admin-badge"> ★ admin</span>}
                        </span>
                        {!isMe && (
                          <div className="member-actions">
                            {!isAdminMember && (
                              <button
                                className="btn btn-sm btn-success"
                                onClick={() => handlePromoteAdmin(m.userIdentity.toHexString())}
                                title="Promote to admin"
                              >
                                Promote
                              </button>
                            )}
                            <button
                              className="btn btn-sm btn-danger"
                              onClick={() => handleKickUser(m.userIdentity.toHexString())}
                              title="Kick user"
                            >
                              Kick
                            </button>
                            <button
                              className="btn btn-sm btn-danger"
                              onClick={() => handleBanUser(m.userIdentity.toHexString())}
                              title="Ban user"
                            >
                              Ban
                            </button>
                          </div>
                        )}
                      </div>
                    );
                  })}
              </div>
            )}

            {/* Invite panel (admin of private rooms) */}
            {amIAdmin && selectedRoom.isPrivate && showInvitePanel && (
              <div className="invite-panel">
                <div className="invite-panel-title">Invite User by Identity</div>
                <div className="invite-panel-row">
                  <select
                    className="input input-sm invite-select"
                    value={inviteIdentityInput}
                    onChange={e => setInviteIdentityInput(e.target.value)}
                  >
                    <option value="">Select user...</option>
                    {users
                      .filter(u => u.identity.toHexString() !== myIdentity?.toHexString())
                      .filter(u => !roomMembers.some(m => m.roomId === selectedRoomId && m.userIdentity.toHexString() === u.identity.toHexString()))
                      .map(u => (
                        <option key={u.identity.toHexString()} value={u.identity.toHexString()}>
                          {u.name}
                        </option>
                      ))
                    }
                  </select>
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={handleInviteUser}
                    disabled={!inviteIdentityInput}
                  >
                    Send Invite
                  </button>
                  <button className="btn btn-sm" onClick={() => setShowInvitePanel(false)}>Cancel</button>
                </div>
              </div>
            )}

            {/* Messages */}
            <div className="messages">
              {roomMessages.length === 0 && (
                <div className="empty-messages">No messages yet. Say hello!</div>
              )}
              {roomMessages.map(msg => {
                const sender = users.find(u => u.identity.toHexString() === msg.sender.toHexString());
                const isMe = msg.sender.toHexString() === myIdentity?.toHexString();
                const seenBy = getSeenBy(msg.id);
                // Only show seen by for the last message in a sequence or if > 0
                const seenByOthers = seenBy.filter(u => u.identity.toHexString() !== msg.sender.toHexString());
                const countdown = getEphemeralCountdown(msg.expiresAtMicros);
                const isEditing = editingMessageId === msg.id;
                const edits = getEditHistory(msg.id);
                const wasEdited = edits.length > 0;

                return (
                  <div key={String(msg.id)} className={`message ${isMe ? 'message-me' : ''} ${countdown !== null ? 'message-ephemeral' : ''}`}>
                    <div className="message-avatar">
                      {(sender?.name ?? '?')[0].toUpperCase()}
                    </div>
                    <div className="message-content">
                      <div className="message-header">
                        <span className="message-sender">{sender?.name ?? 'Unknown'}</span>
                        <span className="message-time">{formatTime(msg.sentAt)}</span>
                        {countdown !== null && (
                          <span className="ephemeral-countdown">{countdown}</span>
                        )}
                        {wasEdited && (
                          <button
                            className="edited-badge"
                            onClick={() => setShowHistoryFor(showHistoryFor === msg.id ? null : msg.id)}
                            title="View edit history"
                          >
                            (edited)
                          </button>
                        )}
                        {isMe && !isEditing && countdown === null && (
                          <button
                            className="edit-btn"
                            onClick={() => handleStartEdit(msg.id, msg.text)}
                            title="Edit message"
                          >
                            ✏️
                          </button>
                        )}
                        <button
                          className="reply-btn"
                          onClick={() => setOpenThreadMessageId(openThreadMessageId === msg.id ? null : msg.id)}
                          title="View/reply thread"
                        >
                          💬{getReplyCount(msg.id) > 0 ? ` ${getReplyCount(msg.id)}` : ''}
                        </button>
                      </div>
                      {isEditing ? (
                        <div className="edit-input-area">
                          <input
                            className="input edit-input"
                            type="text"
                            value={editInput}
                            onChange={e => setEditInput(e.target.value)}
                            onKeyDown={e => {
                              if (e.key === 'Enter') handleSubmitEdit(msg.id);
                              if (e.key === 'Escape') handleCancelEdit();
                            }}
                            maxLength={1000}
                            autoFocus
                          />
                          <button className="btn btn-sm btn-primary" onClick={() => handleSubmitEdit(msg.id)} disabled={!editInput.trim()}>Save</button>
                          <button className="btn btn-sm" onClick={handleCancelEdit}>Cancel</button>
                        </div>
                      ) : (
                        <div className="message-text">{msg.text}</div>
                      )}
                      {showHistoryFor === msg.id && edits.length > 0 && (
                        <div className="edit-history">
                          <div className="edit-history-title">Edit history</div>
                          {edits.map(e => (
                            <div key={String(e.id)} className="edit-history-entry">
                              <span className="edit-history-time">{formatTime(e.editedAt)}</span>
                              <span className="edit-history-text">{e.oldText}</span>
                            </div>
                          ))}
                        </div>
                      )}
                      {/* Reaction bar */}
                      {(() => {
                        const msgReactions = getReactionsForMessage(msg.id);
                        const myHex = myIdentity?.toHexString();
                        return (
                          <div className="reactions">
                            {/* Show existing reaction counts */}
                            {[...msgReactions.entries()].map(([emoji, reactors]) => {
                              const iMine = reactions.some(
                                r => r.messageId === msg.id && r.emoji === emoji &&
                                  r.userIdentity.toHexString() === myHex
                              );
                              return (
                                <button
                                  key={emoji}
                                  className={`reaction-btn ${iMine ? 'reaction-btn-active' : ''}`}
                                  onClick={() => handleToggleReaction(msg.id, emoji)}
                                  title={reactors.join(', ')}
                                >
                                  {emoji} {reactors.length}
                                </button>
                              );
                            })}
                            {/* Emoji picker: show on hover via CSS */}
                            <div className="reaction-picker">
                              {REACTION_EMOJIS.map(emoji => (
                                <button
                                  key={emoji}
                                  className="reaction-picker-btn"
                                  onClick={() => handleToggleReaction(msg.id, emoji)}
                                  title={`React with ${emoji}`}
                                >
                                  {emoji}
                                </button>
                              ))}
                            </div>
                          </div>
                        );
                      })()}
                      {seenByOthers.length > 0 && (
                        <div className="seen-by">
                          Seen by {seenByOthers.map(u => u.name).join(', ')}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            <div className="typing-indicator">
              {typingUsersInRoom.length === 1 && (
                <span>{typingUsersInRoom[0].name} is typing<span className="typing-dots">...</span></span>
              )}
              {typingUsersInRoom.length === 2 && (
                <span>{typingUsersInRoom[0].name} and {typingUsersInRoom[1].name} are typing<span className="typing-dots">...</span></span>
              )}
              {typingUsersInRoom.length > 2 && (
                <span>Multiple users are typing<span className="typing-dots">...</span></span>
              )}
            </div>

            {/* Pending scheduled messages */}
            {myPendingScheduled.length > 0 && (
              <div className="scheduled-pending">
                <div className="scheduled-pending-title">Scheduled ({myPendingScheduled.length})</div>
                {myPendingScheduled.map(sm => (
                  <div key={String(sm.scheduledId)} className="scheduled-item">
                    <span className="scheduled-time">{formatScheduledTime(sm.scheduledAt as any)}</span>
                    <span className="scheduled-text">{sm.text}</span>
                    <button
                      className="btn-icon btn-icon-sm btn-danger"
                      onClick={() => handleCancelScheduled(sm.scheduledId)}
                      title="Cancel scheduled message"
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Scheduler UI */}
            {showScheduler && (
              <div className="scheduler-panel">
                <span className="scheduler-label">Send at:</span>
                <input
                  className="input input-sm scheduler-time"
                  type="datetime-local"
                  value={scheduleTime}
                  min={new Date(Date.now() + 60000).toISOString().slice(0, 16)}
                  onChange={e => setScheduleTime(e.target.value)}
                />
                <button
                  className="btn btn-sm btn-primary"
                  onClick={handleScheduleMessage}
                  disabled={!messageInput.trim() || !scheduleTime}
                >
                  Schedule
                </button>
                <button className="btn btn-sm" onClick={() => setShowScheduler(false)}>
                  Cancel
                </button>
              </div>
            )}

            {/* Banned notice */}
            {amIBanned && (
              <div className="banned-notice">
                You have been banned from this room and cannot send messages.
              </div>
            )}

            {/* Message input */}
            <div className="message-input-area">
              <input
                className="input message-input"
                type="text"
                placeholder={`Message #${selectedRoom.name}...`}
                value={messageInput}
                onChange={e => handleMessageInput(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && !showScheduler && handleSendMessage()}
                maxLength={1000}
              />
              <select
                className="ephemeral-select"
                value={ephemeralDuration}
                onChange={e => setEphemeralDuration(Number(e.target.value))}
                title="Disappear after..."
              >
                <option value={0}>Normal</option>
                <option value={60}>1 min</option>
                <option value={300}>5 min</option>
              </select>
              <button
                className="btn btn-icon schedule-btn"
                onClick={() => {
                  if (!scheduleTime) setScheduleTime(defaultScheduleTime());
                  setShowScheduler(!showScheduler);
                }}
                title="Schedule message"
              >
                🕐
              </button>
              <button
                className="btn btn-primary send-btn"
                onClick={handleSendMessage}
                disabled={!messageInput.trim() || showScheduler || amIBanned}
              >
                Send
              </button>
            </div>
          </>
        ) : (
          <div className="no-room-selected">
            <div className="no-room-content">
              <div className="no-room-icon">⚡</div>
              <h2>Welcome, {myUser.name}!</h2>
              <p>Select a room from the sidebar or create a new one to start chatting.</p>
            </div>
          </div>
        )}
      </div>

      {/* Thread panel */}
      {openThreadMessageId !== null && (() => {
        const parentMsg = messages.find(m => m.id === openThreadMessageId);
        const parentSender = parentMsg ? users.find(u => u.identity.toHexString() === parentMsg.sender.toHexString()) : undefined;
        const replies = getRepliesForMessage(openThreadMessageId);
        return (
          <div className="thread-panel">
            <div className="thread-panel-header">
              <span className="thread-panel-title">Thread</span>
              <button className="btn-icon" onClick={() => setOpenThreadMessageId(null)} title="Close thread">×</button>
            </div>
            <div className="thread-messages">
              {parentMsg && (
                <div className="thread-parent-message">
                  <div className="message-avatar">{(parentSender?.name ?? '?')[0].toUpperCase()}</div>
                  <div className="message-content">
                    <div className="message-header">
                      <span className="message-sender">{parentSender?.name ?? 'Unknown'}</span>
                      <span className="message-time">{formatTime(parentMsg.sentAt)}</span>
                    </div>
                    <div className="message-text">{parentMsg.text}</div>
                  </div>
                </div>
              )}
              <div className="thread-reply-divider">
                {replies.length} {replies.length === 1 ? 'reply' : 'replies'}
              </div>
              {replies.map(reply => {
                const replySender = users.find(u => u.identity.toHexString() === reply.sender.toHexString());
                return (
                  <div key={String(reply.id)} className="thread-reply-message">
                    <div className="message-avatar">{(replySender?.name ?? '?')[0].toUpperCase()}</div>
                    <div className="message-content">
                      <div className="message-header">
                        <span className="message-sender">{replySender?.name ?? 'Unknown'}</span>
                        <span className="message-time">{formatTime(reply.sentAt)}</span>
                      </div>
                      <div className="message-text">{reply.text}</div>
                    </div>
                  </div>
                );
              })}
              {replies.length === 0 && (
                <div className="thread-empty">No replies yet. Start the thread!</div>
              )}
            </div>
            <div className="thread-input-area">
              <input
                className="input thread-input"
                type="text"
                placeholder="Reply to thread..."
                value={threadReplyInput}
                onChange={e => setThreadReplyInput(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSendThreadReply()}
                maxLength={1000}
                autoFocus
              />
              <button
                className="btn btn-primary send-btn"
                onClick={handleSendThreadReply}
                disabled={!threadReplyInput.trim()}
              >
                Reply
              </button>
            </div>
          </div>
        );
      })()}
    </div>
  );
}

export default App;
