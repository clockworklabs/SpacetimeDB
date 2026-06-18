import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, Room, User, UserRoomRead, TypingIndicator, ScheduledMessage, MessageReaction, MessageEditHistory, RoomMember, RoomBan } from './module_bindings/types';

// ---- helpers ----

function tsToMs(ts: { microsSinceUnixEpoch: bigint }): number {
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  return new Date(tsToMs(ts)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function getRemainingSeconds(expiresAtMicros: bigint): number {
  const nowMicros = BigInt(Date.now()) * 1000n;
  const remaining = Number((expiresAtMicros - nowMicros) / 1_000_000n);
  return Math.max(0, remaining);
}

function formatRemaining(s: number): string {
  if (s <= 0) return 'expiring...';
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return sec > 0 ? `${m}m ${sec}s` : `${m}m`;
}

function formatScheduledTime(microsSinceEpoch: bigint): string {
  return new Date(Number(microsSinceEpoch / 1000n)).toLocaleString([], {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  });
}

function toLocalDateTimeMin(): string {
  const d = new Date(Date.now() + 60000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

const NAME_COLORS = ['#4cf490', '#a880ff', '#02befa', '#fbdc8e', '#ff4c4c', '#4cf4d8', '#f490c4'];
function nameColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) & 0xffffff;
  return NAME_COLORS[Math.abs(hash) % NAME_COLORS.length];
}

// ---- Rich presence helpers ----

function statusDotClass(status: string): string {
  switch (status) {
    case 'online': return 'dot-online';
    case 'away': return 'dot-away';
    case 'dnd': return 'dot-dnd';
    default: return 'dot-offline';
  }
}

function formatLastActive(lastActiveAt: { microsSinceUnixEpoch: bigint }): string {
  const diffMs = Date.now() - tsToMs(lastActiveAt);
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return 'Last active just now';
  if (diffMin === 1) return 'Last active 1 minute ago';
  if (diffMin < 60) return `Last active ${diffMin} minutes ago`;
  const diffH = Math.floor(diffMin / 60);
  if (diffH === 1) return 'Last active 1 hour ago';
  return `Last active ${diffH} hours ago`;
}

const TYPING_TIMEOUT_MS = 5000;
const TYPING_DEBOUNCE_MS = 3000;
const AUTO_AWAY_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes of inactivity

// ---- MessageList ----

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

interface MessageListProps {
  messages: readonly Message[];
  users: readonly User[];
  myIdentity: { toHexString(): string } | null | undefined;
  userRoomReads: readonly UserRoomRead[];
  reactions: readonly MessageReaction[];
  editHistory: readonly MessageEditHistory[];
  onToggleReaction: (messageId: bigint, emoji: string) => void;
  onEditMessage: (messageId: bigint, content: string) => void;
}

function MessageList({ messages, users, myIdentity, userRoomReads, reactions, editHistory, onToggleReaction, onEditMessage }: MessageListProps) {
  const [hoverMsgId, setHoverMsgId] = useState<bigint | null>(null);
  const [editingMsgId, setEditingMsgId] = useState<bigint | null>(null);
  const [editContent, setEditContent] = useState('');
  const [historyMsgId, setHistoryMsgId] = useState<bigint | null>(null);

  const getUserByIdentity = (hex: string): User | undefined =>
    users.find(u => u.identity.toHexString() === hex);

  const getExactReaders = (msg: Message, idx: number): User[] => {
    const nextMsg = messages[idx + 1];
    return userRoomReads
      .filter(r => {
        if (r.roomId !== msg.roomId) return false;
        if (r.lastReadMessageId < msg.id) return false;
        if (nextMsg && r.lastReadMessageId >= nextMsg.id) return false;
        return true;
      })
      .map(r => getUserByIdentity(r.userIdentity.toHexString()))
      .filter((u): u is User => u !== undefined)
      .filter(u => u.identity.toHexString() !== msg.sender.toHexString());
  };

  const getReactionGroups = (msgId: bigint): Map<string, MessageReaction[]> => {
    const grouped = new Map<string, MessageReaction[]>();
    for (const r of reactions.filter(r => r.messageId === msgId)) {
      if (!grouped.has(r.emoji)) grouped.set(r.emoji, []);
      grouped.get(r.emoji)!.push(r);
    }
    return grouped;
  };

  const getMsgHistory = (msgId: bigint): MessageEditHistory[] =>
    [...editHistory.filter(h => h.messageId === msgId)]
      .sort((a, b) => {
        const d = a.editedAt.microsSinceUnixEpoch - b.editedAt.microsSinceUnixEpoch;
        return d > 0n ? 1 : d < 0n ? -1 : 0;
      });

  const startEdit = (msg: Message) => {
    setEditingMsgId(msg.id);
    setEditContent(msg.content);
    setHoverMsgId(null);
  };

  const saveEdit = () => {
    if (editingMsgId === null || !editContent.trim()) return;
    onEditMessage(editingMsgId, editContent.trim());
    setEditingMsgId(null);
    setEditContent('');
  };

  const cancelEdit = () => {
    setEditingMsgId(null);
    setEditContent('');
  };

  const myHex = myIdentity?.toHexString();

  type Group = { sender: User | undefined; senderHex: string; msgs: { msg: Message; idx: number }[] };
  const groups: Group[] = [];
  messages.forEach((msg, idx) => {
    const senderHex = msg.sender.toHexString();
    const last = groups[groups.length - 1];
    if (last && last.senderHex === senderHex) {
      last.msgs.push({ msg, idx });
    } else {
      groups.push({
        sender: getUserByIdentity(senderHex),
        senderHex,
        msgs: [{ msg, idx }],
      });
    }
  });

  return (
    <div className="message-list">
      {groups.map(group => (
        <div key={String(group.msgs[0].msg.id)} className="message-group">
          <div className="message-group-header">
            <div
              className="sender-avatar"
              style={{ background: nameColor(group.sender?.name ?? '?') }}
            >
              {(group.sender?.name?.[0] ?? '?').toUpperCase()}
            </div>
            <span className="sender-name" style={{ color: nameColor(group.sender?.name ?? '?') }}>
              {group.sender?.name ?? 'Unknown'}
              {group.sender?.identity.toHexString() === myIdentity?.toHexString() && (
                <span className="you-label"> (you)</span>
              )}
            </span>
            <span className="message-time">{formatTime(group.msgs[0].msg.sentAt)}</span>
          </div>
          {group.msgs.map(({ msg, idx }) => {
            const readers = getExactReaders(msg, idx);
            const isEphemeral = msg.expiresAt !== undefined;
            const reactionGroups = getReactionGroups(msg.id);
            const isHovered = hoverMsgId === msg.id;
            const isMyMsg = msg.sender.toHexString() === myHex;
            const isEditing = editingMsgId === msg.id;
            const msgHistory = getMsgHistory(msg.id);
            const isEdited = msgHistory.length > 0;
            const showHistory = historyMsgId === msg.id;
            return (
              <div
                key={String(msg.id)}
                className={`message-row${isEphemeral ? ' ephemeral-message' : ''}`}
                onMouseEnter={() => { if (!isEditing) setHoverMsgId(msg.id); }}
                onMouseLeave={() => setHoverMsgId(null)}
              >
                {isEditing ? (
                  <div className="edit-form">
                    <input
                      className="edit-input"
                      value={editContent}
                      onChange={e => setEditContent(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); saveEdit(); }
                        if (e.key === 'Escape') cancelEdit();
                      }}
                      maxLength={2000}
                      autoFocus
                    />
                    <button className="btn-primary btn-sm" onClick={saveEdit}>Save</button>
                    <button className="btn-ghost btn-sm" onClick={cancelEdit}>Cancel</button>
                  </div>
                ) : (
                  <div className="message-content-wrapper">
                    <div className="message-content">
                      {msg.content}
                      {isEdited && (
                        <button
                          className="edited-indicator"
                          onClick={() => setHistoryMsgId(showHistory ? null : msg.id)}
                          title="View edit history"
                        >
                          (edited)
                        </button>
                      )}
                    </div>
                    {isHovered && (
                      <div className="reaction-picker">
                        {isMyMsg && (
                          <button
                            className="reaction-pick-btn edit-btn"
                            onClick={() => startEdit(msg)}
                            title="Edit message"
                          >
                            Edit
                          </button>
                        )}
                        {REACTION_EMOJIS.map(emoji => (
                          <button
                            key={emoji}
                            className="reaction-pick-btn"
                            onClick={() => onToggleReaction(msg.id, emoji)}
                            title={`React with ${emoji}`}
                            aria-label={`react with ${emoji}`}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}
                {showHistory && msgHistory.length > 0 && (
                  <div className="edit-history-panel">
                    <div className="edit-history-title">Edit history</div>
                    {msgHistory.map((h, i) => (
                      <div key={String(h.id)} className="edit-history-entry">
                        <span className="edit-history-version">v{i + 1}</span>
                        <span className="edit-history-content">{h.previousContent}</span>
                        <span className="edit-history-time muted small">
                          {formatTime(h.editedAt)}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
                {isEphemeral && (
                  <div className="ephemeral-badge">
                    ⏱ disappears in {formatRemaining(getRemainingSeconds(msg.expiresAt!))}
                  </div>
                )}
                {reactionGroups.size > 0 && (
                  <div className="reaction-row">
                    {[...reactionGroups.entries()].map(([emoji, rs]) => {
                      const iMine = rs.some(r => r.userIdentity.toHexString() === myHex);
                      const names = rs
                        .map(r => getUserByIdentity(r.userIdentity.toHexString())?.name ?? 'Unknown')
                        .join(', ');
                      return (
                        <button
                          key={emoji}
                          className={`reaction-chip${iMine ? ' reaction-mine' : ''}`}
                          onClick={() => onToggleReaction(msg.id, emoji)}
                          title={names}
                        >
                          {emoji} {rs.length}
                        </button>
                      );
                    })}
                  </div>
                )}
                {readers.length > 0 && (
                  <div className="read-receipt">
                    Seen by {readers.map(u => u.name).join(', ')}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ---- ScheduledMessagesList ----

interface ScheduledMessagesListProps {
  pending: readonly ScheduledMessage[];
  onCancel: (id: bigint) => void;
}

function ScheduledMessagesList({ pending, onCancel }: ScheduledMessagesListProps) {
  if (pending.length === 0) return null;
  return (
    <div className="scheduled-panel">
      <div className="scheduled-header">Scheduled ({pending.length})</div>
      {pending.map(sm => (
        <div key={String(sm.id)} className="scheduled-item">
          <div className="scheduled-item-info">
            <span className="scheduled-time">{formatScheduledTime(sm.sendAt)}</span>
            <span className="scheduled-content">{sm.content}</span>
          </div>
          <button
            className="btn-cancel"
            onClick={() => onCancel(sm.id)}
          >
            Cancel
          </button>
        </div>
      ))}
    </div>
  );
}

// ---- App ----

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [nameError, setNameError] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showNewRoom, setShowNewRoom] = useState(false);
  const [roomError, setRoomError] = useState('');
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [typingActive, setTypingActive] = useState(false);
  const [, setTick] = useState(0);

  // Ephemeral message duration (null = permanent, number = TTL in seconds)
  const [ephemeralTtl, setEphemeralTtl] = useState<number | null>(null);

  const [showMembersPanel, setShowMembersPanel] = useState(false);

  // Scheduled messages state
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleContent, setScheduleContent] = useState('');
  const [scheduleDateTime, setScheduleDateTime] = useState('');
  const [scheduleError, setScheduleError] = useState('');

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const selectedRoomIdRef = useRef<bigint | null>(null);
  selectedRoomIdRef.current = selectedRoomId;

  // Rich presence refs (updated each render to avoid stale closures)
  const autoAwaySetRef = useRef(false);
  const lastActivityRef = useRef(Date.now());
  const myUserRef = useRef<User | undefined>(undefined);
  const connRef = useRef<DbConnection | null>(null);

  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive || !myIdentity) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.roomMember,
        tables.roomBan,
        // Semijoin: only messages for rooms where the current user is a member.
        // When kicked (room_member row deleted), messages from that room are removed from the local cache.
        tables.roomMember
          .where(m => m.userIdentity.eq(myIdentity))
          .rightSemijoin(tables.message, (member, msg) => member.roomId.eq(msg.roomId)),
        tables.typingIndicator,
        tables.userRoomRead,
        tables.scheduledMessage,
        tables.messageReaction,
        tables.messageEditHistory,
      ]);
  }, [conn, isActive, myIdentity]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [userRoomReads] = useTable(tables.userRoomRead);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [messageReactions] = useTable(tables.messageReaction);
  const [messageEditHistories] = useTable(tables.messageEditHistory);
  const [roomBans] = useTable(tables.roomBan);

  const myHex = myIdentity?.toHexString();
  const myUser = users.find(u => u.identity.toHexString() === myHex);

  // Keep refs current each render
  myUserRef.current = myUser;
  connRef.current = conn;

  const myMemberships = roomMembers.filter(m => m.userIdentity.toHexString() === myHex);
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));
  const myRooms = rooms
    .filter(r => myRoomIds.has(r.id))
    .sort((a, b) => {
      const d = a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });
  const otherRooms = rooms.filter(r => !myRoomIds.has(r.id));

  // Visible users: not offline and not invisible (invisible appears offline to others)
  const visibleUsers = users.filter(u => u.status !== 'offline' && u.status !== 'invisible' && u.name !== '');
  const onlineCount = users.filter(u => u.status === 'online' && u.name !== '').length;

  const selectedRoom = rooms.find(r => r.id === selectedRoomId);
  const roomMessages = messages
    .filter(m => m.roomId === selectedRoomId)
    .sort((a, b) => {
      const d = a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  // Pending scheduled messages for the current room, authored by me
  const myPendingScheduled = scheduledMessages
    .filter(sm =>
      sm.roomId === selectedRoomId &&
      sm.sender.toHexString() === myHex
    )
    .sort((a, b) => {
      const d = a.sendAt - b.sendAt;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  // Permissions
  const myMembershipInSelected = selectedRoomId
    ? roomMembers.find(m => m.roomId === selectedRoomId && m.userIdentity.toHexString() === myHex)
    : undefined;
  const iAmAdminInSelected = myMembershipInSelected?.isAdmin ?? false;

  const isKickedFromSelectedRoom = selectedRoomId !== null
    && !myRoomIds.has(selectedRoomId)
    && roomBans.some(b => b.roomId === selectedRoomId && b.userIdentity.toHexString() === myHex);

  const selectedRoomMembers = selectedRoomId
    ? roomMembers.filter(m => m.roomId === selectedRoomId)
    : [];

  const getUnreadCount = (roomId: bigint): number => {
    const read = userRoomReads.find(
      r => r.roomId === roomId && r.userIdentity.toHexString() === myHex
    );
    const lastReadId = read?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastReadId).length;
  };

  const typingUsers = selectedRoomId
    ? typingIndicators
        .filter(ti => {
          if (ti.roomId !== selectedRoomId) return false;
          if (ti.userIdentity.toHexString() === myHex) return false;
          return Date.now() - tsToMs(ti.updatedAt) < TYPING_TIMEOUT_MS;
        })
        .map(ti => users.find(u => u.identity.toHexString() === ti.userIdentity.toHexString()))
        .filter((u): u is User => u !== undefined)
    : [];

  useEffect(() => {
    if (!conn || !selectedRoomId || roomMessages.length === 0) return;
    const latest = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: latest.id });
  }, [selectedRoomId, roomMessages.length]);

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  useEffect(() => {
    if (isAtBottom) scrollToBottom();
  }, [roomMessages.length, isAtBottom, scrollToBottom]);

  useEffect(() => {
    setIsAtBottom(true);
    setTimeout(() => messagesEndRef.current?.scrollIntoView(), 50);
    setMessageInput('');
    setShowMembersPanel(false);
  }, [selectedRoomId]);

  const handleScroll = () => {
    const c = messagesContainerRef.current;
    if (!c) return;
    setIsAtBottom(c.scrollHeight - c.scrollTop - c.clientHeight < 60);
  };

  const stopTyping = useCallback(() => {
    if (!conn || !selectedRoomIdRef.current) return;
    conn.reducers.updateTyping({ roomId: selectedRoomIdRef.current, isTyping: false });
    setTypingActive(false);
  }, [conn]);

  const handleTyping = (value: string) => {
    setMessageInput(value);
    if (!conn || !selectedRoomId) return;

    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);

    if (value.length > 0) {
      if (!typingActive) {
        conn.reducers.updateTyping({ roomId: selectedRoomId, isTyping: true });
        setTypingActive(true);
      }
      typingTimerRef.current = setTimeout(stopTyping, TYPING_DEBOUNCE_MS);
    } else {
      stopTyping();
    }
  };

  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      stopTyping();
    };
  }, [selectedRoomId, stopTyping]);

  // ---- Rich presence: activity tracking ----

  // Register global activity listeners once; use refs to avoid stale closures
  useEffect(() => {
    const onActivity = () => {
      lastActivityRef.current = Date.now();
      // If auto-away was triggered, restore to online on next activity
      if (autoAwaySetRef.current && myUserRef.current?.status === 'away') {
        autoAwaySetRef.current = false;
        connRef.current?.reducers.setStatus({ status: 'online' });
      }
    };
    window.addEventListener('keydown', onActivity);
    window.addEventListener('mousemove', onActivity);
    return () => {
      window.removeEventListener('keydown', onActivity);
      window.removeEventListener('mousemove', onActivity);
    };
  }, []);

  // Auto-away timer: check every 30s, set away after AUTO_AWAY_TIMEOUT_MS of inactivity
  useEffect(() => {
    const interval = setInterval(() => {
      const user = myUserRef.current;
      const connection = connRef.current;
      if (!user || user.status !== 'online' || !connection) return;
      const idleMs = Date.now() - lastActivityRef.current;
      if (idleMs >= AUTO_AWAY_TIMEOUT_MS) {
        autoAwaySetRef.current = true;
        connection.reducers.setStatus({ status: 'away' });
      }
    }, 30_000);
    return () => clearInterval(interval);
  }, []);

  // ---- handlers ----

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    conn.reducers.sendMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      ttlSeconds: ephemeralTtl !== null ? ephemeralTtl : undefined,
    });
    setMessageInput('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    stopTyping();
    setIsAtBottom(true);
  };

  const handleSetName = () => {
    if (!conn || !nameInput.trim()) { setNameError('Please enter a display name'); return; }
    conn.reducers.setName({ name: nameInput.trim() });
    setNameError('');
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) { setRoomError('Please enter a room name'); return; }
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowNewRoom(false);
    setRoomError('');
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
    setSelectedRoomId(roomId);
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  };

  const openScheduleModal = () => {
    setScheduleContent(messageInput);
    setScheduleDateTime(toLocalDateTimeMin());
    setScheduleError('');
    setShowScheduleModal(true);
  };

  const handleScheduleMessage = () => {
    if (!conn || !selectedRoomId) return;
    if (!scheduleContent.trim()) { setScheduleError('Message cannot be empty'); return; }
    if (!scheduleDateTime) { setScheduleError('Please select a send time'); return; }
    const sendAtMs = new Date(scheduleDateTime).getTime();
    if (isNaN(sendAtMs) || sendAtMs <= Date.now()) {
      setScheduleError('Scheduled time must be in the future');
      return;
    }
    const sendAt = BigInt(sendAtMs) * 1000n;
    conn.reducers.scheduleMessage({ roomId: selectedRoomId, content: scheduleContent.trim(), sendAt });
    setShowScheduleModal(false);
    setMessageInput('');
    setScheduleContent('');
    setScheduleError('');
  };

  const handleCancelScheduled = (id: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ id });
  };

  const handleToggleReaction = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    conn.reducers.toggleReaction({ messageId, emoji });
  };

  const handleEditMessage = (messageId: bigint, content: string) => {
    if (!conn) return;
    conn.reducers.editMessage({ messageId, content });
  };

  const handleKickUser = (memberId: bigint) => {
    if (!conn) return;
    conn.reducers.kickUser({ memberId });
  };

  const handlePromoteToAdmin = (memberId: bigint) => {
    if (!conn) return;
    conn.reducers.promoteToAdmin({ memberId });
  };

  const handleSetStatus = (status: string) => {
    if (!conn) return;
    autoAwaySetRef.current = false; // manual change clears auto-away flag
    conn.reducers.setStatus({ status });
  };

  // ---- Connecting screen ----
  if (!isActive || !subscribed) {
    return (
      <div className="fullscreen-center">
        <div className="connect-card">
          <div className="spinner" />
          <h2 className="gradient-title">SpacetimeDB Chat</h2>
          <p className="muted">Connecting to server...</p>
        </div>
      </div>
    );
  }

  // ---- Name setup screen ----
  if (!myUser || myUser.name === '') {
    return (
      <div className="fullscreen-center">
        <div className="connect-card">
          <h1 className="gradient-title">SpacetimeDB Chat</h1>
          <p className="muted">Choose a display name to get started</p>
          <div className="name-input-row">
            <input
              type="text"
              placeholder="Your display name..."
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSetName()}
              maxLength={32}
              autoFocus
            />
            <button className="btn-primary" onClick={handleSetName}>
              Join
            </button>
          </div>
          {nameError && <p className="error-msg">{nameError}</p>}
        </div>
      </div>
    );
  }

  // ---- Main UI ----
  return (
    <div className="app-layout">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-brand">
          <span className="gradient-title brand-title">SpacetimeDB Chat</span>
        </div>

        <div className="sidebar-me">
          <div className="avatar" style={{ background: nameColor(myUser.name) }}>
            {myUser.name[0].toUpperCase()}
          </div>
          <div className="sidebar-me-info">
            <span className="sidebar-me-name">{myUser.name}</span>
            <div className="status-row">
              <span className={`dot ${statusDotClass(myUser.status)}`} />
              <select
                className="status-select"
                value={myUser.status === 'offline' ? 'online' : myUser.status}
                onChange={e => handleSetStatus(e.target.value)}
                title="Set your status"
              >
                <option value="online">Online</option>
                <option value="away">Away</option>
                <option value="dnd">Do Not Disturb</option>
                <option value="invisible">Invisible</option>
              </select>
            </div>
          </div>
        </div>

        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowNewRoom(true)} title="Create room">
              +
            </button>
          </div>

          {myRooms.length === 0 && (
            <p className="empty-hint">No rooms yet — create one!</p>
          )}

          {myRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <button
                key={String(room.id)}
                className={`room-btn ${selectedRoomId === room.id ? 'active' : ''}`}
                onClick={() => setSelectedRoomId(room.id)}
              >
                <span className="room-hash">#</span>
                <span className="room-btn-name">{room.name}</span>
                {unread > 0 && <span className="badge">{unread}</span>}
              </button>
            );
          })}

          {otherRooms.length > 0 && (
            <>
              <div className="subsection-label">Other Rooms</div>
              {otherRooms.map(room => (
                <div key={String(room.id)} className="room-btn other-room">
                  <span className="room-hash">#</span>
                  <span className="room-btn-name">{room.name}</span>
                  <button className="join-btn" onClick={() => handleJoinRoom(room.id)}>
                    Join
                  </button>
                </div>
              ))}
            </>
          )}
        </div>

        <div className="sidebar-section online-section">
          <div className="section-header">
            <span>Online — {onlineCount}</span>
          </div>
          {visibleUsers.map(u => (
            <div key={u.identity.toHexString()} className="online-user">
              <span className={`dot ${statusDotClass(u.status)}`} />
              <div className="online-user-info">
                <span className={u.identity.toHexString() === myHex ? 'font-bold' : ''}>
                  {u.name}
                  {u.identity.toHexString() === myHex && <span className="muted small"> (you)</span>}
                </span>
                {(u.status === 'away' || u.status === 'offline') && (
                  <span className="last-active-text">{formatLastActive(u.lastActiveAt)}</span>
                )}
              </div>
            </div>
          ))}
        </div>
      </aside>

      {/* Main */}
      <main className="chat-main">
        {!selectedRoom ? (
          <div className="fullscreen-center flex-1">
            <div className="welcome-card">
              <h2>Welcome, {myUser.name}!</h2>
              <p className="muted">Select a room from the sidebar or create a new one.</p>
              <button className="btn-primary" onClick={() => setShowNewRoom(true)}>
                Create a Room
              </button>
            </div>
          </div>
        ) : (
          <div className="chat-layout">
            {/* Header */}
            <div className="chat-header">
              <div className="chat-header-left">
                <span className="room-hash-lg">#</span>
                <h2 className="chat-room-title">{selectedRoom.name}</h2>
              </div>
              <div className="chat-header-actions">
                <button
                  className={`btn-ghost${showMembersPanel ? ' btn-active' : ''}`}
                  onClick={() => setShowMembersPanel(p => !p)}
                >
                  Members
                </button>
                <button
                  className="btn-ghost"
                  onClick={() => handleLeaveRoom(selectedRoom.id)}
                >
                  Leave
                </button>
              </div>
            </div>

            {isKickedFromSelectedRoom ? (
              <div className="kicked-overlay">
                <div className="kicked-card">
                  <p className="kicked-text">You have been kicked from this room.</p>
                  <button className="btn-primary" onClick={() => setSelectedRoomId(null)}>
                    Dismiss
                  </button>
                </div>
              </div>
            ) : (
              <>
                {/* Members panel */}
                {showMembersPanel && (
                  <div className="members-panel">
                    <div className="members-panel-header">
                      <span>Members ({selectedRoomMembers.length})</span>
                      <button className="icon-btn" onClick={() => setShowMembersPanel(false)}>✕</button>
                    </div>
                    {selectedRoomMembers.map(member => {
                      const memberUser = users.find(u => u.identity.toHexString() === member.userIdentity.toHexString());
                      const isMe = member.userIdentity.toHexString() === myHex;
                      const memberStatus = memberUser?.status ?? 'offline';
                      return (
                        <div key={String(member.id)} className="member-row">
                          <div className="member-info">
                            <span className={`dot ${statusDotClass(memberStatus)}`} />
                            <div className="member-name-col">
                              <span className="member-name" style={{ color: nameColor(memberUser?.name ?? '?') }}>
                                {memberUser?.name ?? 'Unknown'}
                                {isMe && <span className="muted small"> (you)</span>}
                              </span>
                              {(memberStatus === 'away' || memberStatus === 'offline') && memberUser && (
                                <span className="last-active-text">{formatLastActive(memberUser.lastActiveAt)}</span>
                              )}
                            </div>
                            {member.isAdmin && <span className="admin-badge">Admin</span>}
                          </div>
                          {iAmAdminInSelected && !isMe && !member.isAdmin && (
                            <div className="member-actions">
                              <button
                                className="btn-danger btn-sm"
                                onClick={() => handleKickUser(member.id)}
                              >
                                Kick
                              </button>
                              <button
                                className="btn-ghost btn-sm"
                                onClick={() => handlePromoteToAdmin(member.id)}
                              >
                                Promote
                              </button>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}

                {/* Messages */}
                <div
                  ref={messagesContainerRef}
                  className="messages-area"
                  onScroll={handleScroll}
                >
                  {roomMessages.length === 0 ? (
                    <div className="fullscreen-center flex-1">
                      <p className="muted">No messages yet — say something!</p>
                    </div>
                  ) : (
                    <MessageList
                      messages={roomMessages}
                      users={users}
                      myIdentity={myIdentity}
                      userRoomReads={userRoomReads}
                      reactions={messageReactions}
                      editHistory={messageEditHistories}
                      onToggleReaction={handleToggleReaction}
                      onEditMessage={handleEditMessage}
                    />
                  )}
                  <div ref={messagesEndRef} />
                </div>

                {/* Scroll to bottom */}
                {!isAtBottom && (
                  <button
                    className="scroll-btn"
                    onClick={() => { scrollToBottom(); setIsAtBottom(true); }}
                  >
                    ↓ Scroll to latest
                  </button>
                )}

                {/* Pending scheduled messages for this room */}
                <ScheduledMessagesList
                  pending={myPendingScheduled}
                  onCancel={handleCancelScheduled}
                />

                {/* Typing indicator */}
                <div className="typing-row">
                  {typingUsers.length > 0 && (
                    <span className="typing-text">
                      {typingUsers.length === 1
                        ? `${typingUsers[0].name} is typing...`
                        : `${typingUsers.map(u => u.name).join(', ')} are typing...`}
                    </span>
                  )}
                </div>

                {/* Input bar */}
                <div className="input-bar">
                  <input
                    type="text"
                    className="message-input"
                    placeholder={`Message #${selectedRoom.name}`}
                    value={messageInput}
                    onChange={e => handleTyping(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        handleSendMessage();
                      }
                    }}
                    maxLength={2000}
                  />
                  <select
                    className={`ephemeral-select${ephemeralTtl !== null ? ' ephemeral-active' : ''}`}
                    value={ephemeralTtl ?? ''}
                    onChange={e => setEphemeralTtl(e.target.value ? Number(e.target.value) : null)}
                    title="Disappearing message duration"
                  >
                    <option value="">No expiry</option>
                    <option value="60">⏱ 1 min</option>
                    <option value="300">⏱ 5 min</option>
                    <option value="600">⏱ 10 min</option>
                  </select>
                  <button
                    className="btn-ghost"
                    onClick={openScheduleModal}
                    title="Schedule message"
                    aria-label="schedule message"
                  >
                    Schedule
                  </button>
                  <button
                    className="btn-primary"
                    onClick={handleSendMessage}
                    disabled={!messageInput.trim()}
                  >
                    Send
                  </button>
                </div>
              </>
            )}
          </div>
        )}
      </main>

      {/* New Room Modal */}
      {showNewRoom && (
        <div
          className="modal-backdrop"
          onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Create a Room</h3>
            <input
              type="text"
              placeholder="Room name..."
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') handleCreateRoom();
                if (e.key === 'Escape') { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }
              }}
              maxLength={64}
              autoFocus
            />
            {roomError && <p className="error-msg">{roomError}</p>}
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); }}
              >
                Cancel
              </button>
              <button className="btn-primary" onClick={handleCreateRoom}>
                Create
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Schedule Message Modal */}
      {showScheduleModal && (
        <div
          className="modal-backdrop"
          onClick={() => { setShowScheduleModal(false); setScheduleError(''); }}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Schedule a Message</h3>
            <input
              type="text"
              placeholder="Message content..."
              value={scheduleContent}
              onChange={e => setScheduleContent(e.target.value)}
              maxLength={2000}
              autoFocus
            />
            <label className="schedule-label">
              <span className="muted small">Send at</span>
              <input
                type="datetime-local"
                value={scheduleDateTime}
                min={toLocalDateTimeMin()}
                onChange={e => setScheduleDateTime(e.target.value)}
                className="datetime-input"
              />
            </label>
            {scheduleError && <p className="error-msg">{scheduleError}</p>}
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => { setShowScheduleModal(false); setScheduleError(''); }}
              >
                Cancel
              </button>
              <button
                className="btn-primary"
                onClick={handleScheduleMessage}
                disabled={!scheduleContent.trim() || !scheduleDateTime}
              >
                Schedule
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
