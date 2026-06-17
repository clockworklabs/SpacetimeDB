import { useState, useEffect, useRef } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message } from './module_bindings/types';
import type { Identity } from 'spacetimedb';

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

const TYPING_EXPIRY_US = 3_000_000n;

const EPHEMERAL_OPTIONS = [
  { label: '30s', seconds: 30 },
  { label: '1m', seconds: 60 },
  { label: '5m', seconds: 300 },
  { label: '1h', seconds: 3600 },
];

const INACTIVITY_MS = 5 * 60 * 1000; // 5 minutes

const STATUS_LABELS: Record<string, string> = {
  online: 'Online',
  away: 'Away',
  dnd: 'Do Not Disturb',
  invisible: 'Invisible',
};

function formatTime(microsSinceUnixEpoch: bigint): string {
  const ms = Number(microsSinceUnixEpoch / 1000n);
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatCountdown(expiresAtUs: bigint): string {
  const nowUs = BigInt(Date.now()) * 1000n;
  const remainingUs = expiresAtUs - nowUs;
  if (remainingUs <= 0n) return 'expiring...';
  const s = Number(remainingUs / 1_000_000n);
  if (s >= 3600) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  if (s >= 60) return `${Math.floor(s / 60)}m ${s % 60}s`;
  return `${s}s`;
}

function formatScheduledTime(scheduledAt: { tag: string; value: bigint } | { microsSinceUnixEpoch: bigint } | unknown): string {
  try {
    const sa = scheduledAt as Record<string, unknown>;
    let us: bigint | undefined;
    if (sa.tag === 'time' && typeof sa.value === 'bigint') {
      us = sa.value;
    } else if (typeof (sa as { microsSinceUnixEpoch?: unknown }).microsSinceUnixEpoch === 'bigint') {
      us = (sa as { microsSinceUnixEpoch: bigint }).microsSinceUnixEpoch;
    }
    if (us !== undefined) {
      const ms = Number(us / 1000n);
      return new Date(ms).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    }
  } catch {
    // ignore
  }
  return '(unknown time)';
}

function formatLastActive(lastActiveAt: { microsSinceUnixEpoch: bigint } | undefined | null): string {
  if (!lastActiveAt) return 'a while ago';
  const ms = Number(lastActiveAt.microsSinceUnixEpoch / 1000n);
  const diffMs = Date.now() - ms;
  const diffMins = Math.floor(diffMs / 60_000);
  if (diffMins < 1) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  return `${diffDays}d ago`;
}

function identityHex(identity: { toHexString: () => string }): string {
  return identity.toHexString();
}

function getMinDatetimeLocal(): string {
  const now = new Date(Date.now() + 60_000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
}

function getStatusClass(status: string, online: boolean): string {
  if (!online) return 'status-offline';
  const s = status || 'online';
  return `status-${s}`;
}

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [messageReactions] = useTable(tables.messageReaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [roomPermissions] = useTable(tables.roomPermission);

  const [subscribed, setSubscribed] = useState(false);
  const [currentRoomId, setCurrentRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleDateTime, setScheduleDateTime] = useState('');
  const [ephemeralTtl, setEphemeralTtl] = useState<number | null>(null);
  const [showEphemeral, setShowEphemeral] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editingText, setEditingText] = useState('');
  const [viewingHistoryForId, setViewingHistoryForId] = useState<bigint | null>(null);
  const [showMembersPanel, setShowMembersPanel] = useState(false);
  const [kickedFromRoom, setKickedFromRoom] = useState<bigint | null>(null);
  const [showStatusPicker, setShowStatusPicker] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const lastTypingRef = useRef<number>(0);
  const subscribedRef = useRef(false);
  const connRef = useRef<DbConnection | null>(null);
  const isActiveRef = useRef(false);
  const myStatusRef = useRef<string>('online');
  const inactivityTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Force re-render every second for typing indicator expiry and last-active times
  const [, forceRefresh] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => forceRefresh(n => n + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => { connRef.current = conn; }, [conn]);
  useEffect(() => { isActiveRef.current = isActive; }, [isActive]);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive || subscribedRef.current) return;
    subscribedRef.current = true;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.roomMember,
        tables.message,
        tables.typingIndicator,
        tables.readReceipt,
        tables.scheduledMessage,
        tables.messageReaction,
        tables.messageEdit,
        tables.roomPermission,
      ]);
  }, [conn, isActive]);

  // Inactivity detection — auto-set to away after INACTIVITY_MS of no activity
  useEffect(() => {
    function onActivity() {
      if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
      // Come back online if we were auto-set to away
      if (myStatusRef.current === 'away' && connRef.current && isActiveRef.current) {
        connRef.current.reducers.setStatus({ status: 'online' });
      }
      inactivityTimerRef.current = setTimeout(() => {
        if (connRef.current && isActiveRef.current && myStatusRef.current === 'online') {
          connRef.current.reducers.setStatus({ status: 'away' });
        }
      }, INACTIVITY_MS);
    }
    const events = ['mousemove', 'keypress', 'click', 'scroll'] as const;
    events.forEach(e => document.addEventListener(e, onActivity, { passive: true }));
    onActivity();
    return () => {
      events.forEach(e => document.removeEventListener(e, onActivity));
      if (inactivityTimerRef.current) clearTimeout(inactivityTimerRef.current);
    };
  }, []);

  // Close status picker on outside click
  useEffect(() => {
    if (!showStatusPicker) return;
    const close = () => setShowStatusPicker(false);
    document.addEventListener('click', close);
    return () => document.removeEventListener('click', close);
  }, [showStatusPicker]);

  const myHex = myIdentity ? identityHex(myIdentity) : null;

  // Detect when the current user gets kicked from the current room
  useEffect(() => {
    if (!currentRoomId || !myHex) return;
    const myPerm = roomPermissions.find(
      p => p.roomId === currentRoomId && identityHex(p.userIdentity) === myHex
    );
    if (myPerm?.isBanned) {
      setKickedFromRoom(currentRoomId);
      setCurrentRoomId(null);
      setShowMembersPanel(false);
    }
  }, [roomPermissions, currentRoomId, myHex]);

  const myUser = users.find(u => identityHex(u.identity) === myHex);

  // Keep status ref in sync with server state
  useEffect(() => {
    myStatusRef.current = myUser?.status || 'online';
  }, [myUser?.status]);

  const hasName = !!(myUser?.name);

  const myMemberRoomIds = new Set(
    roomMembers
      .filter(m => identityHex(m.userIdentity) === myHex)
      .map(m => m.roomId)
  );

  const myRooms = rooms.filter(r => myMemberRoomIds.has(r.id));
  const otherRooms = rooms.filter(r => !myMemberRoomIds.has(r.id));

  const knownUsers = users.filter(u => u.name);
  // Visible online users: connected and not invisible (invisible users appear offline to others)
  const visibleOnlineUsers = knownUsers.filter(u => {
    const isMe = identityHex(u.identity) === myHex;
    if (isMe) return u.online;
    return u.online && u.status !== 'invisible';
  });
  // Offline or invisible users (excluding self)
  const offlineUsers = knownUsers.filter(u => {
    const isMe = identityHex(u.identity) === myHex;
    if (isMe) return false;
    return !u.online || u.status === 'invisible';
  });

  const currentMessages = messages
    .filter(m => m.roomId === currentRoomId)
    .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

  const nowUs = BigInt(Date.now()) * 1000n;
  const currentTyping = typingIndicators.filter(
    ti =>
      ti.roomId === currentRoomId &&
      identityHex(ti.userIdentity) !== myHex &&
      nowUs - ti.updatedAt.microsSinceUnixEpoch < TYPING_EXPIRY_US
  );

  const myPendingScheduled = scheduledMessages.filter(
    sm => sm.roomId === currentRoomId && identityHex(sm.senderIdentity) === myHex
  );

  function getUnreadCount(roomId: bigint): number {
    const roomMsgs = messages.filter(m => m.roomId === roomId);
    if (roomMsgs.length === 0) return 0;
    const myReceipt = readReceipts.find(
      r => r.roomId === roomId && identityHex(r.userIdentity) === myHex
    );
    if (!myReceipt) return roomMsgs.length;
    return roomMsgs.filter(m => m.id > myReceipt.lastReadMessageId).length;
  }

  function getSeenBy(msg: Message): string[] {
    return readReceipts
      .filter(
        r =>
          r.roomId === msg.roomId &&
          r.lastReadMessageId >= msg.id &&
          identityHex(r.userIdentity) !== identityHex(msg.senderIdentity)
      )
      .map(r => users.find(u => identityHex(u.identity) === identityHex(r.userIdentity))?.name)
      .filter((n): n is string => !!n);
  }

  // Auto-scroll when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'instant' });
  }, [currentMessages.length, currentRoomId]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!currentRoomId || !conn || !isActive || !subscribed || currentMessages.length === 0) return;
    const lastMsg = currentMessages[currentMessages.length - 1];
    conn.reducers.markRead({ roomId: currentRoomId, messageId: lastMsg.id });
  }, [currentRoomId, currentMessages.length, conn, isActive, subscribed]);

  function isAdminInRoom(roomId: bigint, identity: { toHexString: () => string }): boolean {
    return roomPermissions.some(
      p => p.roomId === roomId && identityHex(p.userIdentity) === identityHex(identity) && p.isAdmin
    );
  }

  function handleKickUser(roomId: bigint, targetIdentity: Identity) {
    if (!conn || !isActive) return;
    conn.reducers.kickUser({ roomId, targetIdentity });
  }

  function handlePromoteUser(roomId: bigint, targetIdentity: Identity) {
    if (!conn || !isActive) return;
    conn.reducers.promoteUser({ roomId, targetIdentity });
  }

  function handleSetStatus(status: string) {
    if (!conn || !isActive) return;
    conn.reducers.setStatus({ status });
    setShowStatusPicker(false);
  }

  function handleSetName(e: React.FormEvent) {
    e.preventDefault();
    if (!nameInput.trim() || !conn || !isActive) return;
    conn.reducers.setName({ name: nameInput.trim() });
    setNameInput('');
  }

  function handleCreateRoom(e: React.FormEvent) {
    e.preventDefault();
    if (!newRoomName.trim() || !conn || !isActive) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowCreateRoom(false);
  }

  function handleJoinRoom(roomId: bigint) {
    if (!conn || !isActive) return;
    conn.reducers.joinRoom({ roomId });
    setCurrentRoomId(roomId);
  }

  function handleLeaveRoom(e: React.MouseEvent, roomId: bigint) {
    e.stopPropagation();
    if (!conn || !isActive) return;
    conn.reducers.leaveRoom({ roomId });
    if (currentRoomId === roomId) setCurrentRoomId(null);
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!messageInput.trim() || !currentRoomId || !conn || !isActive) return;
    if (ephemeralTtl !== null) {
      conn.reducers.sendEphemeralMessage({ roomId: currentRoomId, text: messageInput.trim(), ttlSeconds: ephemeralTtl });
    } else {
      conn.reducers.sendMessage({ roomId: currentRoomId, text: messageInput.trim() });
    }
    setMessageInput('');
  }

  function handleScheduleMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!messageInput.trim() || !currentRoomId || !conn || !isActive || !scheduleDateTime) return;
    const scheduledAtMs = new Date(scheduleDateTime).getTime();
    if (isNaN(scheduledAtMs) || scheduledAtMs <= Date.now()) return;
    const scheduledAtUs = BigInt(Math.floor(scheduledAtMs)) * 1000n;
    conn.reducers.scheduleMessage({ roomId: currentRoomId, text: messageInput.trim(), scheduledAtUs });
    setMessageInput('');
    setShowSchedule(false);
    setScheduleDateTime('');
  }

  function handleCancelScheduled(scheduledId: bigint) {
    if (!conn || !isActive) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  }

  function handleToggleReaction(messageId: bigint, emoji: string) {
    if (!conn || !isActive) return;
    conn.reducers.toggleReaction({ messageId, emoji });
  }

  function handleStartEdit(msg: Message) {
    setEditingMessageId(msg.id);
    setEditingText(msg.text);
    setViewingHistoryForId(null);
  }

  function handleCancelEdit() {
    setEditingMessageId(null);
    setEditingText('');
  }

  function handleEditMessage(e: React.FormEvent, messageId: bigint) {
    e.preventDefault();
    if (!editingText.trim() || !conn || !isActive) return;
    conn.reducers.editMessage({ messageId, newText: editingText.trim() });
    setEditingMessageId(null);
    setEditingText('');
  }

  function toggleHistory(msgId: bigint) {
    setViewingHistoryForId(prev => prev === msgId ? null : msgId);
  }

  function getReactionGroups(messageId: bigint): { emoji: string; count: number; reactorNames: string[]; iMine: boolean }[] {
    const reactions = messageReactions.filter(r => r.messageId === messageId);
    const grouped = new Map<string, { count: number; reactorNames: string[]; iMine: boolean }>();
    for (const r of reactions) {
      const hex = identityHex(r.userIdentity);
      const name = users.find(u => identityHex(u.identity) === hex)?.name ?? 'Unknown';
      const isMine = hex === myHex;
      if (!grouped.has(r.emoji)) grouped.set(r.emoji, { count: 0, reactorNames: [], iMine: false });
      const g = grouped.get(r.emoji)!;
      g.count++;
      g.reactorNames.push(name);
      if (isMine) g.iMine = true;
    }
    return [...grouped.entries()].map(([emoji, g]) => ({ emoji, ...g }));
  }

  function handleInputChange(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageInput(e.target.value);
    const now = Date.now();
    if (currentRoomId && conn && isActive && now - lastTypingRef.current > 1000) {
      lastTypingRef.current = now;
      conn.reducers.setTyping({ roomId: currentRoomId });
    }
  }

  const currentRoom = rooms.find(r => r.id === currentRoomId);
  const myCurrentStatus = myUser?.status || 'online';

  // Loading / connecting state
  if (!isActive || !subscribed) {
    return (
      <div className="app">
        <div className="loading">
          <div className="loading-text">Connecting to SpacetimeDB...</div>
        </div>
      </div>
    );
  }

  // Name setup
  if (!hasName) {
    return (
      <div className="app">
        <div className="name-setup">
          <div className="name-setup-card">
            <div className="app-title-large">SpacetimeDB Chat</div>
            <div className="name-setup-subtitle">Choose a display name to get started</div>
            <form onSubmit={handleSetName}>
              <input
                className="name-input"
                type="text"
                placeholder="Your name..."
                value={nameInput}
                onChange={e => setNameInput(e.target.value)}
                maxLength={32}
                autoFocus
              />
              <button className="primary-btn" type="submit" disabled={!nameInput.trim()}>
                Join Chat
              </button>
            </form>
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
          <div className="app-title">SpacetimeDB Chat</div>
        </div>

        <div className="user-info" style={{ position: 'relative' }}>
          <div
            className={`status-dot ${getStatusClass(myCurrentStatus, true)}`}
            onClick={e => { e.stopPropagation(); setShowStatusPicker(s => !s); }}
            style={{ cursor: 'pointer' }}
            title={`Status: ${STATUS_LABELS[myCurrentStatus] ?? myCurrentStatus} — click to change`}
          />
          <span className="username">{myUser?.name}</span>
          {showStatusPicker && (
            <div className="status-picker" onClick={e => e.stopPropagation()}>
              {(['online', 'away', 'dnd', 'invisible'] as const).map(s => (
                <button
                  key={s}
                  className={`status-picker-option${myCurrentStatus === s ? ' active' : ''}`}
                  onClick={() => handleSetStatus(s)}
                >
                  <div className={`status-dot status-${s}`} />
                  <span>{STATUS_LABELS[s]}</span>
                </button>
              ))}
            </div>
          )}
        </div>

        {/* My Rooms */}
        <div className="section rooms-section">
          <div className="section-title">Rooms</div>
          {myRooms.length === 0 && (
            <div className="empty-section-text">No rooms yet</div>
          )}
          {myRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <div
                key={String(room.id)}
                className={`room-item ${currentRoomId === room.id ? 'active' : ''}`}
                onClick={() => { setCurrentRoomId(room.id); setShowMembersPanel(false); setKickedFromRoom(null); }}
              >
                <span className="room-name"># {room.name}</span>
                <div className="room-item-actions">
                  {unread > 0 && currentRoomId !== room.id && (
                    <span className="unread-badge">{unread}</span>
                  )}
                  <button
                    className="leave-btn"
                    onClick={e => handleLeaveRoom(e, room.id)}
                    title="Leave room"
                  >
                    ×
                  </button>
                </div>
              </div>
            );
          })}
          <button className="add-btn" onClick={() => setShowCreateRoom(true)}>
            + Create Room
          </button>
        </div>

        {/* Other Rooms */}
        {otherRooms.length > 0 && (
          <div className="section">
            <div className="section-title">Browse Rooms</div>
            {otherRooms.map(room => (
              <div key={String(room.id)} className="room-item browse-room">
                <span className="room-name"># {room.name}</span>
                <button
                  className="join-btn"
                  onClick={() => handleJoinRoom(room.id)}
                >
                  Join
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Users / Presence */}
        <div className="section online-section">
          <div className="section-title">Online — {visibleOnlineUsers.length}</div>
          {visibleOnlineUsers.map(u => {
            const isMe = identityHex(u.identity) === myHex;
            const status = u.status || 'online';
            const showLastActive = status === 'away' || status === 'dnd';
            return (
              <div key={identityHex(u.identity)} className="user-item">
                <div
                  className={`status-dot ${getStatusClass(status, true)}`}
                  title={STATUS_LABELS[status] ?? status}
                />
                <span className="user-item-name">
                  {u.name}{isMe ? ' (you)' : ''}
                </span>
                {showLastActive && u.lastActiveAt && (
                  <span className="user-last-active">{formatLastActive(u.lastActiveAt)}</span>
                )}
              </div>
            );
          })}
          {offlineUsers.length > 0 && (
            <>
              <div className="section-title" style={{ marginTop: '8px' }}>
                Offline — {offlineUsers.length}
              </div>
              {offlineUsers.map(u => (
                <div key={identityHex(u.identity)} className="user-item user-item-offline">
                  <div className="status-dot status-offline" title="Offline" />
                  <span className="user-item-name">{u.name}</span>
                  {u.lastActiveAt && (
                    <span className="user-last-active">{formatLastActive(u.lastActiveAt)}</span>
                  )}
                </div>
              ))}
            </>
          )}
        </div>
      </div>

      {/* Main Area */}
      <div className="main">
        {kickedFromRoom !== null && !currentRoom && (
          <div className="kicked-notice">
            You have been kicked from this room.
          </div>
        )}
        {!currentRoom && kickedFromRoom === null ? (
          <div className="empty-state">
            <div className="empty-state-title">Welcome, {myUser?.name}!</div>
            <div className="empty-state-sub">Select a room or create one to start chatting</div>
          </div>
        ) : currentRoom ? (
          <>
            {/* Room Header */}
            <div className="room-header">
              <span className="room-header-name"># {currentRoom.name}</span>
              <span className="room-member-count">
                {roomMembers.filter(m => m.roomId === currentRoom.id).length} members
              </span>
              <button
                className="members-btn"
                onClick={() => setShowMembersPanel(s => !s)}
              >
                Members
              </button>
            </div>

            {/* Message List */}
            <div className="message-list">
              {currentMessages.length === 0 && (
                <div className="empty-messages">
                  No messages yet. Be the first to say something!
                </div>
              )}
              {currentMessages.map((msg, i) => {
                const prev = i > 0 ? currentMessages[i - 1] : null;
                const isGrouped =
                  prev &&
                  identityHex(prev.senderIdentity) === identityHex(msg.senderIdentity) &&
                  msg.sentAt.microsSinceUnixEpoch - prev.sentAt.microsSinceUnixEpoch < 60_000_000n;
                const sender = users.find(u => identityHex(u.identity) === identityHex(msg.senderIdentity));
                const senderName = sender?.name ?? 'Unknown';
                const isMe = identityHex(msg.senderIdentity) === myHex;
                const seenBy = getSeenBy(msg);
                const reactionGroups = getReactionGroups(msg.id);
                const isEditing = editingMessageId === msg.id;
                const isEdited = msg.editedAt !== undefined && msg.editedAt !== null;
                const showHistory = viewingHistoryForId === msg.id;
                const msgEditHistory = messageEdits
                  .filter(e => e.messageId === msg.id)
                  .sort((a, b) => (a.editedAt.microsSinceUnixEpoch < b.editedAt.microsSinceUnixEpoch ? -1 : 1));

                return (
                  <div key={String(msg.id)} className={`message-wrapper ${isMe ? 'mine' : ''}`}>
                    {!isGrouped && (
                      <div className="message-header">
                        <span className={`message-sender ${isMe ? 'sender-me' : ''}`}>
                          {senderName}
                        </span>
                        <span className="message-time">
                          {formatTime(msg.sentAt.microsSinceUnixEpoch)}
                        </span>
                        {isMe && !isEditing && (
                          <button
                            className="edit-btn"
                            onClick={() => handleStartEdit(msg)}
                            title="Edit message"
                          >
                            ✎
                          </button>
                        )}
                      </div>
                    )}
                    {isEditing ? (
                      <form className="edit-form" onSubmit={e => handleEditMessage(e, msg.id)}>
                        <input
                          className="edit-input"
                          type="text"
                          value={editingText}
                          onChange={e => setEditingText(e.target.value)}
                          maxLength={2000}
                          autoFocus
                        />
                        <button className="edit-confirm-btn" type="submit" disabled={!editingText.trim()}>
                          Save
                        </button>
                        <button className="edit-cancel-btn" type="button" onClick={handleCancelEdit}>
                          Cancel
                        </button>
                      </form>
                    ) : (
                      <div className={isGrouped ? 'message-grouped-text' : 'message-text'}>
                        {isGrouped && (
                          <span className="grouped-time">
                            {formatTime(msg.sentAt.microsSinceUnixEpoch)}
                          </span>
                        )}
                        {isGrouped && isMe && (
                          <button
                            className="edit-btn edit-btn-grouped"
                            onClick={() => handleStartEdit(msg)}
                            title="Edit message"
                          >
                            ✎
                          </button>
                        )}
                        {msg.text}
                        {isEdited && (
                          <button
                            className="edited-tag"
                            onClick={() => toggleHistory(msg.id)}
                            title="View edit history"
                          >
                            (edited)
                          </button>
                        )}
                      </div>
                    )}
                    {showHistory && msgEditHistory.length > 0 && (
                      <div className="edit-history">
                        <div className="edit-history-title">Edit history</div>
                        {msgEditHistory.map((edit, idx) => (
                          <div key={String(edit.id)} className="edit-history-item">
                            <span className="edit-history-version">v{idx + 1}</span>
                            <span className="edit-history-text">{edit.previousText}</span>
                            <span className="edit-history-time">
                              {formatTime(edit.editedAt.microsSinceUnixEpoch)}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                    {msg.expiresAtUs !== undefined && (
                      <div className="ephemeral-countdown">
                        ⏱ {formatCountdown(msg.expiresAtUs)}
                      </div>
                    )}
                    {seenBy.length > 0 && (
                      <div className="message-seen">
                        Seen by {seenBy.join(', ')}
                      </div>
                    )}
                    <div className="message-reactions-row">
                      {reactionGroups.map(({ emoji, count, reactorNames, iMine }) => (
                        <button
                          key={emoji}
                          className={`reaction-chip${iMine ? ' mine' : ''}`}
                          onClick={() => handleToggleReaction(msg.id, emoji)}
                          title={reactorNames.join(', ')}
                        >
                          {emoji} {count}
                        </button>
                      ))}
                      <div className="reaction-add-wrapper">
                        <span className="reaction-add-btn">+</span>
                        <div className="reaction-picker">
                          {REACTION_EMOJIS.map(e => (
                            <button
                              key={e}
                              className="reaction-picker-emoji"
                              onClick={() => handleToggleReaction(msg.id, e)}
                            >
                              {e}
                            </button>
                          ))}
                        </div>
                      </div>
                    </div>
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Pending Scheduled Messages */}
            {myPendingScheduled.length > 0 && (
              <div className="scheduled-list">
                <div className="scheduled-list-title">Scheduled ({myPendingScheduled.length})</div>
                {myPendingScheduled.map(sm => (
                  <div key={String(sm.scheduledId)} className="scheduled-item">
                    <span className="scheduled-text">{sm.text}</span>
                    <span className="scheduled-time">{formatScheduledTime(sm.scheduledAt)}</span>
                    <button
                      className="cancel-scheduled-btn"
                      onClick={() => handleCancelScheduled(sm.scheduledId)}
                      title="Cancel scheduled message"
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Input Area */}
            <div className="input-area">
              <div className="typing-indicator">
                {currentTyping.length === 1 && (
                  <>
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[0].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' is typing...'}
                  </>
                )}
                {currentTyping.length === 2 && (
                  <>
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[0].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' and '}
                    <span className="typing-name">
                      {users.find(u => identityHex(u.identity) === identityHex(currentTyping[1].userIdentity))?.name ?? 'Someone'}
                    </span>
                    {' are typing...'}
                  </>
                )}
                {currentTyping.length > 2 && 'Multiple users are typing...'}
              </div>

              {showEphemeral && (
                <div className="ephemeral-picker">
                  <span className="ephemeral-label">Disappears after:</span>
                  {EPHEMERAL_OPTIONS.map(({ label, seconds }) => (
                    <button
                      key={label}
                      type="button"
                      className={`ephemeral-option${ephemeralTtl === seconds ? ' selected' : ''}`}
                      onClick={() => { setEphemeralTtl(ephemeralTtl === seconds ? null : seconds); }}
                    >
                      {label}
                    </button>
                  ))}
                  {ephemeralTtl !== null && (
                    <button
                      type="button"
                      className="ephemeral-option"
                      onClick={() => setEphemeralTtl(null)}
                    >
                      off
                    </button>
                  )}
                </div>
              )}

              {showSchedule && (
                <form className="schedule-form" onSubmit={handleScheduleMessage}>
                  <input
                    className="schedule-datetime"
                    type="datetime-local"
                    value={scheduleDateTime}
                    min={getMinDatetimeLocal()}
                    onChange={e => setScheduleDateTime(e.target.value)}
                    required
                  />
                  <button
                    className="schedule-confirm-btn"
                    type="submit"
                    disabled={!messageInput.trim() || !scheduleDateTime}
                  >
                    Schedule Send
                  </button>
                  <button
                    type="button"
                    className="schedule-cancel-btn"
                    onClick={() => { setShowSchedule(false); setScheduleDateTime(''); }}
                  >
                    Cancel
                  </button>
                </form>
              )}

              <form className="message-form" onSubmit={handleSendMessage}>
                <input
                  className="message-input"
                  type="text"
                  placeholder={`Message #${currentRoom.name}`}
                  value={messageInput}
                  onChange={handleInputChange}
                  maxLength={2000}
                  autoFocus
                />
                <button
                  className={`ephemeral-toggle-btn${ephemeralTtl !== null ? ' active' : ''}`}
                  type="button"
                  title={ephemeralTtl !== null ? `Ephemeral: auto-deletes` : 'Send ephemeral message'}
                  onClick={() => setShowEphemeral(s => !s)}
                >
                  &#128293;
                </button>
                <button
                  className="schedule-toggle-btn"
                  type="button"
                  title="Schedule message"
                  onClick={() => setShowSchedule(s => !s)}
                >
                  &#128337;
                </button>
                <button
                  className="send-btn"
                  type="submit"
                  disabled={!messageInput.trim()}
                >
                  {ephemeralTtl !== null ? `Send ⏱` : 'Send'}
                </button>
              </form>
            </div>

            {/* Members Panel */}
            {showMembersPanel && currentRoom && (() => {
              const members = roomMembers.filter(m => m.roomId === currentRoom.id);
              const amIAdmin = myHex ? isAdminInRoom(currentRoom.id, { toHexString: () => myHex }) : false;
              return (
                <div className="members-panel">
                  <div className="members-panel-header">
                    <span className="members-panel-title">Members</span>
                    <button className="members-panel-close" onClick={() => setShowMembersPanel(false)}>×</button>
                  </div>
                  {members.map(m => {
                    const user = users.find(u => identityHex(u.identity) === identityHex(m.userIdentity));
                    const isMe = identityHex(m.userIdentity) === myHex;
                    const isAdmin = isAdminInRoom(currentRoom.id, m.userIdentity);
                    const memberStatus = user?.status || 'online';
                    const memberOnline = user?.online ?? false;
                    return (
                      <div key={String(m.id)} className="members-panel-item">
                        <div
                          className={`status-dot ${getStatusClass(memberStatus, memberOnline)}`}
                          title={memberOnline ? (STATUS_LABELS[memberStatus] ?? memberStatus) : 'Offline'}
                        />
                        <span className="member-name">{user?.name ?? 'Unknown'}{isMe ? ' (you)' : ''}</span>
                        {isAdmin && <span className="admin-badge">Admin</span>}
                        {amIAdmin && !isMe && !isAdmin && (
                          <>
                            <button
                              className="kick-btn"
                              onClick={() => handleKickUser(currentRoom.id, m.userIdentity)}
                            >
                              Kick
                            </button>
                            <button
                              className="promote-btn"
                              onClick={() => handlePromoteUser(currentRoom.id, m.userIdentity)}
                            >
                              Promote
                            </button>
                          </>
                        )}
                      </div>
                    );
                  })}
                </div>
              );
            })()}
          </>
        ) : null}
      </div>

      {/* Create Room Modal */}
      {showCreateRoom && (
        <div className="modal-overlay" onClick={() => setShowCreateRoom(false)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <div className="modal-title">Create a Room</div>
            <form onSubmit={handleCreateRoom}>
              <input
                className="modal-input"
                type="text"
                placeholder="Room name..."
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                maxLength={32}
                autoFocus
              />
              <div className="modal-actions">
                <button
                  type="button"
                  className="cancel-btn"
                  onClick={() => setShowCreateRoom(false)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="primary-btn modal-submit"
                  disabled={!newRoomName.trim()}
                >
                  Create
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
