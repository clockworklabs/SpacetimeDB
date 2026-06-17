import React, { useEffect, useRef, useState, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, MessageEdit, ScheduledMessage, MessageReaction, RoomAdmin } from './module_bindings/types';
import type { Identity } from 'spacetimedb';
// ── helpers ──────────────────────────────────────────────────────────────────

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  const date = new Date(Number(ts.microsSinceUnixEpoch / 1000n));
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function idHex(id: Identity): string {
  return id.toHexString();
}

function colorForName(name: string): string {
  const colors = [
    '#4cf490', '#a880ff', '#02befa', '#fbdc8e',
    '#ff4c4c', '#4cf4d4', '#f490cf', '#90c8f4',
  ];
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
  return colors[Math.abs(hash) % colors.length];
}

function getStatusClass(
  user: { online: boolean; status: string },
  viewerIsMe: boolean
): string {
  if (!user.online) return 'offline';
  const s = user.status || 'online';
  if (s === 'invisible') return viewerIsMe ? 'invisible' : 'offline';
  return s; // 'online' | 'away' | 'dnd'
}

function formatLastActive(lastActiveAt: { microsSinceUnixEpoch: bigint }): string {
  const us = lastActiveAt.microsSinceUnixEpoch;
  if (us === 0n) return 'Last active unknown';
  const diffMs = Date.now() - Number(us / 1000n);
  if (diffMs < 0) return 'Last active just now';
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return 'Last active just now';
  if (diffMins < 60) return `Last active ${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `Last active ${diffHours}h ago`;
  return `Last active ${Math.floor(diffHours / 24)}d ago`;
}

function getStatusLabel(status: string): string {
  switch (status) {
    case 'away': return 'Away';
    case 'dnd': return 'Do Not Disturb';
    case 'invisible': return 'Invisible';
    default: return 'Online';
  }
}

function getScheduledDate(scheduledAt: ScheduledMessage['scheduledAt']): Date {
  const sa = scheduledAt as any;
  if (sa?.tag === 'Time') {
    return new Date(Number(sa.value.microsSinceUnixEpoch / 1000n));
  }
  return new Date();
}

function formatScheduledAt(scheduledAt: ScheduledMessage['scheduledAt']): string {
  const date = getScheduledDate(scheduledAt);
  return date.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

// ── constants ─────────────────────────────────────────────────────────────────

const EMOJI_OPTIONS = ['👍', '❤️', '😂', '😮', '😢'];

// ── Ephemeral countdown ───────────────────────────────────────────────────────

function EphemeralCountdown({ expiresAtMicros }: { expiresAtMicros: bigint }) {
  const [remaining, setRemaining] = useState(() => {
    const ms = Number(expiresAtMicros / 1000n) - Date.now();
    return Math.max(0, Math.ceil(ms / 1000));
  });

  useEffect(() => {
    const tick = () => {
      const ms = Number(expiresAtMicros / 1000n) - Date.now();
      setRemaining(Math.max(0, Math.ceil(ms / 1000)));
    };
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, [expiresAtMicros]);

  if (remaining <= 0) return <span className="ephemeral-indicator">disappearing…</span>;

  const mins = Math.floor(remaining / 60);
  const secs = remaining % 60;
  const label = mins > 0 ? `${mins}m ${secs}s` : `${secs}s`;
  return <span className="ephemeral-indicator">⏱ expires in {label}</span>;
}

// ── Message reactions component ───────────────────────────────────────────────

function MessageReactions({
  messageId,
  reactions,
  myIdentity,
  users,
  onToggle,
}: {
  messageId: bigint;
  reactions: MessageReaction[];
  myIdentity: Identity | null;
  users: readonly { identity: Identity; name: string; online: boolean; status: string; lastActiveAt: { microsSinceUnixEpoch: bigint } }[];
  onToggle: (messageId: bigint, emoji: string) => void;
}) {
  const [showPicker, setShowPicker] = useState(false);

  // Group reactions by emoji
  const grouped: Record<string, MessageReaction[]> = {};
  for (const r of reactions) {
    if (!grouped[r.emoji]) grouped[r.emoji] = [];
    grouped[r.emoji].push(r);
  }

  function whoReacted(reactionList: MessageReaction[]): string {
    return reactionList
      .map(r => users.find(u => idHex(u.identity) === idHex(r.userIdentity))?.name ?? '???')
      .join(', ');
  }

  function iMine(reactionList: MessageReaction[]): boolean {
    if (!myIdentity) return false;
    return reactionList.some(r => idHex(r.userIdentity) === idHex(myIdentity));
  }

  return (
    <div className="reactions-row">
      {Object.entries(grouped).map(([emoji, list]) => (
        <button
          key={emoji}
          className={`reaction-pill${iMine(list) ? ' mine' : ''}`}
          onClick={() => onToggle(messageId, emoji)}
          title={whoReacted(list)}
        >
          {emoji} <span className="reaction-count">{list.length}</span>
        </button>
      ))}
      <button
        className="reaction-add-btn"
        onClick={() => setShowPicker(p => !p)}
        title="Add reaction"
      >
        +
      </button>
      {showPicker && (
        <div className="reaction-picker">
          {EMOJI_OPTIONS.map(emoji => (
            <button
              key={emoji}
              className="reaction-picker-emoji"
              onClick={() => { onToggle(messageId, emoji); setShowPicker(false); }}
            >
              {emoji}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Members panel ─────────────────────────────────────────────────────────────

function MembersPanel({
  roomId,
  memberships,
  roomAdmins,
  users,
  myIdentity,
  isAdmin,
  onKick,
  onPromote,
  onClose,
}: {
  roomId: bigint;
  memberships: readonly { id: bigint; roomId: bigint; userIdentity: Identity }[];
  roomAdmins: readonly RoomAdmin[];
  users: readonly { identity: Identity; name: string; online: boolean; status: string; lastActiveAt: { microsSinceUnixEpoch: bigint } }[];
  myIdentity: Identity | null;
  isAdmin: boolean;
  onKick: (roomId: bigint, userIdentity: Identity) => void;
  onPromote: (roomId: bigint, userIdentity: Identity) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) { if (e.key === 'Escape') onClose(); }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const adminIds = new Set(
    roomAdmins.filter(a => a.roomId === roomId).map(a => idHex(a.userIdentity))
  );
  const roomMembers = memberships.filter(m => m.roomId === roomId);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal members-modal" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">Members ({roomMembers.length})</h2>
        <div className="members-list">
          {roomMembers.map(m => {
            const user = users.find(u => idHex(u.identity) === idHex(m.userIdentity));
            const isMe = myIdentity && idHex(m.userIdentity) === idHex(myIdentity);
            const memberIsAdmin = adminIds.has(idHex(m.userIdentity));
            return (
              <div key={String(m.id)} className="member-row">
                <span className={`status-dot ${user ? getStatusClass(user, !!isMe) : 'offline'}`} title={user ? getStatusLabel(user.status) : ''} />
                <span className="member-name" style={{ color: colorForName(user?.name ?? '?') }}>
                  {user?.name ?? 'Unknown'}
                  {isMe ? ' (you)' : ''}
                </span>
                {memberIsAdmin && <span className="admin-badge">Admin</span>}
                <span style={{ flex: 1 }} />
                {isAdmin && !isMe && !memberIsAdmin && (
                  <div className="member-actions">
                    <button
                      className="btn btn-tiny"
                      onClick={() => onPromote(roomId, m.userIdentity)}
                    >
                      Promote
                    </button>
                    <button
                      className="btn btn-tiny btn-danger"
                      onClick={() => { onKick(roomId, m.userIdentity); onClose(); }}
                    >
                      Kick
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <div className="modal-actions">
          <button className="btn btn-ghost" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

// ── Setup screen ─────────────────────────────────────────────────────────────

function SetupScreen({ onSetName }: { onSetName: (name: string) => void }) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) { setError('Name is required'); return; }
    if (trimmed.length > 32) { setError('Name too long'); return; }
    onSetName(trimmed);
  }

  return (
    <div className="setup-screen">
      <div className="setup-card">
        <h1 className="app-title">SpacetimeDB Chat</h1>
        <p className="setup-subtitle">Enter your display name to join</p>
        <form onSubmit={handleSubmit} className="setup-form">
          <input
            className="input"
            type="text"
            placeholder="Enter your name"
            value={name}
            onChange={e => { setName(e.target.value); setError(''); }}
            autoFocus
          />
          {error && <p className="error-text">{error}</p>}
          <button type="submit" className="btn btn-primary">
            Join
          </button>
        </form>
      </div>
    </div>
  );
}

// ── Create room modal ─────────────────────────────────────────────────────────

function CreateRoomModal({ onClose, onCreate }: {
  onClose: () => void;
  onCreate: (name: string) => void;
}) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) { setError('Room name is required'); return; }
    onCreate(trimmed);
    onClose();
  }

  useEffect(() => {
    function onKey(e: KeyboardEvent) { if (e.key === 'Escape') onClose(); }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">Create Room</h2>
        <form onSubmit={handleSubmit}>
          <input
            className="input"
            type="text"
            placeholder="Room name"
            value={name}
            onChange={e => { setName(e.target.value); setError(''); }}
            autoFocus
          />
          {error && <p className="error-text">{error}</p>}
          <div className="modal-actions">
            <button type="button" className="btn btn-ghost" onClick={onClose}>Cancel</button>
            <button type="submit" className="btn btn-primary">Create</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ── Schedule message modal ────────────────────────────────────────────────────

function ScheduleMessageModal({ onClose, onSchedule }: {
  onClose: () => void;
  onSchedule: (text: string, scheduledAtMicros: bigint) => void;
}) {
  const [text, setText] = useState('');
  const [scheduledAt, setScheduledAt] = useState('');
  const [error, setError] = useState('');

  // Default to 5 minutes from now
  useEffect(() => {
    const d = new Date(Date.now() + 5 * 60 * 1000);
    // Format for datetime-local: YYYY-MM-DDTHH:mm
    const pad = (n: number) => String(n).padStart(2, '0');
    const local = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
    setScheduledAt(local);
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) { if (e.key === 'Escape') onClose(); }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = text.trim();
    if (!trimmed) { setError('Message cannot be empty'); return; }
    if (!scheduledAt) { setError('Please pick a time'); return; }
    const ms = new Date(scheduledAt).getTime();
    if (isNaN(ms)) { setError('Invalid date/time'); return; }
    if (ms <= Date.now()) { setError('Scheduled time must be in the future'); return; }
    onSchedule(trimmed, BigInt(ms) * 1000n);
    onClose();
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h2 className="modal-title">Schedule Message</h2>
        <form onSubmit={handleSubmit}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            <textarea
              className="input"
              placeholder="Message text"
              value={text}
              onChange={e => { setText(e.target.value); setError(''); }}
              rows={3}
              style={{ resize: 'vertical', fontFamily: 'inherit', fontSize: '14px' }}
              autoFocus
            />
            <div>
              <label style={{ display: 'block', fontSize: '12px', color: 'var(--text-muted)', marginBottom: '6px' }}>
                Send at
              </label>
              <input
                className="input"
                type="datetime-local"
                value={scheduledAt}
                onChange={e => { setScheduledAt(e.target.value); setError(''); }}
              />
            </div>
            {error && <p className="error-text">{error}</p>}
          </div>
          <div className="modal-actions">
            <button type="button" className="btn btn-ghost" onClick={onClose}>Cancel</button>
            <button type="submit" className="btn btn-primary">Schedule</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ── Main App ──────────────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);
  const [activeRoomId, setActiveRoomId] = useState<bigint | null>(null);
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [messageText, setMessageText] = useState('');
  const [hasSetName, setHasSetName] = useState(false);
  const [isScrolledUp, setIsScrolledUp] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState<number | null>(null);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<bigint | null>(null);
  const [showMembersPanel, setShowMembersPanel] = useState(false);
  const [threadParentId, setThreadParentId] = useState<bigint | null>(null);
  const [threadReplyText, setThreadReplyText] = useState('');

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.user,
        tables.room,
        tables.membership,
        tables.message,
        tables.typingIndicator,
        tables.readReceipt,
        tables.scheduledMessage,
        tables.messageReaction,
        tables.messageEdit,
        tables.roomAdmin,
        tables.roomBan,
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [memberships] = useTable(tables.membership);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [scheduledMessages] = useTable(tables.scheduledMessage);
  const [messageReactions] = useTable(tables.messageReaction);
  const [messageEdits] = useTable(tables.messageEdit);
  const [roomAdmins] = useTable(tables.roomAdmin);
  const [roomBans] = useTable(tables.roomBan);

  const myUser = myIdentity ? users.find(u => idHex(u.identity) === idHex(myIdentity)) : undefined;
  const myMemberships = myIdentity
    ? memberships.filter(m => idHex(m.userIdentity) === idHex(myIdentity))
    : [];
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));
  const joinedRooms = rooms.filter(r => myRoomIds.has(r.id));
  const otherRooms = rooms.filter(r => !myRoomIds.has(r.id));

  const activeRoomMessages = activeRoomId
    ? messages
        .filter(m => m.roomId === activeRoomId && (m.parentMessageId === undefined || m.parentMessageId === null))
        .sort((a, b) => (a.sentAt.microsSinceUnixEpoch < b.sentAt.microsSinceUnixEpoch ? -1 : 1))
    : [];

  const threadMessages = threadParentId !== null
    ? messages
        .filter(m => m.parentMessageId === threadParentId)
        .sort((a, b) => (a.sentAt.microsSinceUnixEpoch < b.sentAt.microsSinceUnixEpoch ? -1 : 1))
    : [];

  const threadParentMsg = threadParentId !== null
    ? messages.find(m => m.id === threadParentId)
    : null;

  // Permissions
  const amAdminInActiveRoom = !!(myIdentity && activeRoomId &&
    roomAdmins.some(a => a.roomId === activeRoomId && idHex(a.userIdentity) === idHex(myIdentity)));

  const myBannedRoomIds = new Set(
    myIdentity ? roomBans.filter(b => idHex(b.userIdentity) === idHex(myIdentity)).map(b => b.roomId) : []
  );

  // Pending scheduled messages for the current user in the active room
  const myPendingScheduled = myIdentity && activeRoomId
    ? scheduledMessages
        .filter(sm => idHex(sm.senderIdentity) === idHex(myIdentity) && sm.roomId === activeRoomId)
        .sort((a, b) => {
          const aDate = getScheduledDate(a.scheduledAt);
          const bDate = getScheduledDate(b.scheduledAt);
          return aDate.getTime() - bDate.getTime();
        })
    : [];

  // ── unread counts ───────────────────────────────────────────────────────────

  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const receipt = readReceipts.find(
      r => r.roomId === roomId && idHex(r.userIdentity) === idHex(myIdentity)
    );
    const roomMessages = messages.filter(m => m.roomId === roomId);
    if (!receipt) return roomMessages.length;
    return roomMessages.filter(m => m.id > receipt.lastReadMessageId).length;
  }

  // ── mark read when room is active ───────────────────────────────────────────

  useEffect(() => {
    if (!activeRoomId || !conn || !myIdentity) return;
    const roomMsgs = messages.filter(m => m.roomId === activeRoomId);
    if (roomMsgs.length === 0) return;
    const maxId = roomMsgs.reduce((acc, m) => (m.id > acc ? m.id : acc), 0n);
    const existing = readReceipts.find(
      r => r.roomId === activeRoomId && idHex(r.userIdentity) === idHex(myIdentity)
    );
    if (!existing || existing.lastReadMessageId < maxId) {
      conn.reducers.markRead({ roomId: activeRoomId, lastReadMessageId: maxId });
    }
  }, [activeRoomId, messages, readReceipts, conn, myIdentity]);

  // ── kick detection ──────────────────────────────────────────────────────────
  // If the user is no longer a member of the active room (got kicked), leave the room view

  useEffect(() => {
    if (!myIdentity || !activeRoomId || !subscribed) return;
    const stillMember = memberships.some(
      m => m.roomId === activeRoomId && idHex(m.userIdentity) === idHex(myIdentity)
    );
    if (!stillMember) {
      setActiveRoomId(null);
      setShowMembersPanel(false);
      setThreadParentId(null);
    }
  }, [memberships, myIdentity, activeRoomId, subscribed]);

  // ── auto-scroll ─────────────────────────────────────────────────────────────

  useEffect(() => {
    if (!isScrolledUp) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [activeRoomMessages.length, isScrolledUp]);

  function handleScroll() {
    const el = messagesContainerRef.current;
    if (!el) return;
    const threshold = 100;
    setIsScrolledUp(el.scrollHeight - el.scrollTop - el.clientHeight > threshold);
  }

  // ── typing ──────────────────────────────────────────────────────────────────

  const sendTypingStop = useCallback(() => {
    if (isTypingRef.current && conn && activeRoomId) {
      conn.reducers.setTyping({ roomId: activeRoomId, isTyping: false });
      isTypingRef.current = false;
    }
  }, [conn, activeRoomId]);

  function handleMessageInput(e: React.ChangeEvent<HTMLInputElement>) {
    setMessageText(e.target.value);
    if (!conn || !activeRoomId) return;
    if (!isTypingRef.current) {
      conn.reducers.setTyping({ roomId: activeRoomId, isTyping: true });
      isTypingRef.current = true;
    }
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      sendTypingStop();
    }, 4000);
  }

  // ── typing expiry (client-side for display) ─────────────────────────────────
  // The server stores timestamps; filter indicators older than 5 seconds

  const now = Date.now();
  const activeTypers = activeRoomId
    ? typingIndicators.filter(ti => {
        if (ti.roomId !== activeRoomId) return false;
        if (myIdentity && idHex(ti.userIdentity) === idHex(myIdentity)) return false;
        const age = now - Number(ti.updatedAt.microsSinceUnixEpoch / 1000n);
        return age < 5000;
      })
    : [];

  // ── read receipts display ───────────────────────────────────────────────────

  function getReadersForMessage(msg: Message): string[] {
    if (!myIdentity) return [];
    return readReceipts
      .filter(r => r.roomId === msg.roomId && r.lastReadMessageId >= msg.id && idHex(r.userIdentity) !== idHex(myIdentity))
      .map(r => {
        const user = users.find(u => idHex(u.identity) === idHex(r.userIdentity));
        return user?.name ?? '???';
      });
  }

  function getReplyCount(messageId: bigint): number {
    return messages.filter(m => m.parentMessageId === messageId).length;
  }

  function getFirstReplyPreview(messageId: bigint): string | null {
    const replies = messages
      .filter(m => m.parentMessageId === messageId)
      .sort((a, b) => (a.sentAt.microsSinceUnixEpoch < b.sentAt.microsSinceUnixEpoch ? -1 : 1));
    if (replies.length === 0) return null;
    const first = replies[0];
    const sender = users.find(u => idHex(u.identity) === idHex(first.senderIdentity));
    return `${sender?.name ?? 'Unknown'}: ${first.text.slice(0, 60)}${first.text.length > 60 ? '…' : ''}`;
  }

  // ── actions ─────────────────────────────────────────────────────────────────

  function handleSetName(name: string) {
    conn?.reducers.setName({ name });
    setHasSetName(true);
  }

  function handleCreateRoom(name: string) {
    conn?.reducers.createRoom({ name });
  }

  function handleJoinRoom(roomId: bigint) {
    conn?.reducers.joinRoom({ roomId });
    setActiveRoomId(roomId);
  }

  function handleLeaveRoom(roomId: bigint) {
    conn?.reducers.leaveRoom({ roomId });
    if (activeRoomId === roomId) setActiveRoomId(null);
  }

  function handleSelectRoom(roomId: bigint) {
    setActiveRoomId(roomId);
    setIsScrolledUp(false);
    setThreadParentId(null);
  }

  function handleSendMessage(e: React.FormEvent) {
    e.preventDefault();
    if (!activeRoomId || !messageText.trim()) return;
    if (ephemeralDuration) {
      conn?.reducers.sendEphemeralMessage({ roomId: activeRoomId, text: messageText.trim(), expirySeconds: ephemeralDuration });
    } else {
      conn?.reducers.sendMessage({ roomId: activeRoomId, text: messageText.trim() });
    }
    setMessageText('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    sendTypingStop();
    setIsScrolledUp(false);
  }

  function handleScheduleMessage(text: string, scheduledAtMicros: bigint) {
    if (!activeRoomId) return;
    conn?.reducers.scheduleMessage({ roomId: activeRoomId, text, scheduledAtMicros });
  }

  function handleCancelScheduled(scheduledId: bigint) {
    conn?.reducers.cancelScheduledMessage({ scheduledId });
  }

  function handleToggleReaction(messageId: bigint, emoji: string) {
    conn?.reducers.toggleReaction({ messageId, emoji });
  }

  function handleSetStatus(status: string) {
    conn?.reducers.setStatus({ status });
  }

  function handleKickUser(roomId: bigint, userIdentity: Identity) {
    conn?.reducers.kickUser({ roomId, userIdentity });
  }

  function handlePromoteToAdmin(roomId: bigint, userIdentity: Identity) {
    conn?.reducers.promoteToAdmin({ roomId, userIdentity });
  }

  function handleOpenThread(messageId: bigint) {
    setThreadParentId(messageId);
    setThreadReplyText('');
  }

  function handleCloseThread() {
    setThreadParentId(null);
    setThreadReplyText('');
  }

  function handleSendReply(e: React.FormEvent) {
    e.preventDefault();
    if (!threadParentId || !threadReplyText.trim()) return;
    conn?.reducers.replyToMessage({ parentMessageId: threadParentId, text: threadReplyText.trim() });
    setThreadReplyText('');
  }

  function handleSaveEdit(e: React.FormEvent) {
    e.preventDefault();
    if (!editingMessageId || !editText.trim()) return;
    conn?.reducers.editMessage({ messageId: editingMessageId, newText: editText.trim() });
    setEditingMessageId(null);
    setEditText('');
  }

  function startEditing(msg: Message) {
    setEditingMessageId(msg.id);
    setEditText(msg.text);
  }

  function cancelEditing() {
    setEditingMessageId(null);
    setEditText('');
  }

  // ── loading / setup state ───────────────────────────────────────────────────

  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        <p>Connecting to SpacetimeDB…</p>
      </div>
    );
  }

  if (!myUser && !hasSetName) {
    return <SetupScreen onSetName={handleSetName} />;
  }

  const activeRoom = rooms.find(r => r.id === activeRoomId);

  // ── render ──────────────────────────────────────────────────────────────────

  return (
    <div className="app">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1 className="app-title">SpacetimeDB Chat</h1>
          {myUser && (
            <div className="my-user">
              <span className={`status-dot ${getStatusClass(myUser, true)}`} />
              <span className="user-name" style={{ color: colorForName(myUser.name) }}>
                {myUser.name}
              </span>
              <select
                className="status-selector"
                value={myUser.status || 'online'}
                onChange={e => handleSetStatus(e.target.value)}
                title="Set your status"
                aria-label="Set status"
              >
                <option value="online">Online</option>
                <option value="away">Away</option>
                <option value="dnd">Do Not Disturb</option>
                <option value="invisible">Invisible</option>
              </select>
            </div>
          )}
        </div>

        {/* Room list */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="btn btn-icon" onClick={() => setShowCreateRoom(true)} title="Create room">+</button>
          </div>

          {joinedRooms.length === 0 && otherRooms.length === 0 && (
            <p className="empty-state">Create a room to get started</p>
          )}

          {joinedRooms.map(room => {
            const unread = getUnreadCount(room.id);
            return (
              <div
                key={String(room.id)}
                className={`room-item ${activeRoomId === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room.id)}
              >
                <span className="room-prefix">#</span>
                <span className="room-name">{room.name}</span>
                {unread > 0 && <span className="badge">{unread}</span>}
              </div>
            );
          })}

          {otherRooms.length > 0 && (
            <>
              <div className="sidebar-subsection">Other rooms</div>
              {otherRooms.map(room => {
                const isBanned = myBannedRoomIds.has(room.id);
                return (
                  <div key={String(room.id)} className="room-item room-item-other">
                    <span className="room-prefix">#</span>
                    <span className="room-name">{room.name}</span>
                    {isBanned ? (
                      <span className="banned-badge">Banned</span>
                    ) : (
                      <button
                        className="btn btn-tiny"
                        onClick={e => { e.stopPropagation(); handleJoinRoom(room.id); }}
                      >
                        Join
                      </button>
                    )}
                  </div>
                );
              })}
            </>
          )}
        </div>

        {/* Users */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Users — {users.length}</span>
          </div>
          {users.map(u => {
            const isMe = !!(myIdentity && idHex(u.identity) === idHex(myIdentity));
            const statusCls = getStatusClass(u, isMe);
            const showOffline = !u.online || (u.status === 'invisible' && !isMe);
            return (
              <div key={idHex(u.identity)} className="user-item">
                <span className={`status-dot ${statusCls}`} title={isMe ? getStatusLabel(u.status) : ''} />
                <div className="user-item-info">
                  <span className="user-name" style={{ color: colorForName(u.name) }}>
                    {u.name}{isMe ? ' (you)' : ''}
                  </span>
                  {showOffline && (
                    <span className="last-active">{formatLastActive(u.lastActiveAt)}</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </aside>

      {/* Main area */}
      <main className="main" style={{ display: 'flex', flexDirection: 'row', overflow: 'hidden' }}>
        {!activeRoom ? (
          <div className="empty-main">
            <p className="empty-state">Select a room to start chatting</p>
          </div>
        ) : (
          <div style={{ display: 'flex', flex: 1, overflow: 'hidden' }}>
          <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
            {/* Room header */}
            <header className="room-header">
              <div className="room-header-left">
                <span className="room-prefix">#</span>
                <h2 className="room-header-name">{activeRoom.name}</h2>
                {amAdminInActiveRoom && <span className="admin-badge">Admin</span>}
              </div>
              <div style={{ display: 'flex', gap: '8px' }}>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setShowMembersPanel(true)}
                >
                  Members
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => handleLeaveRoom(activeRoom.id)}
                >
                  Leave
                </button>
              </div>
            </header>

            {/* Messages */}
            <div
              className="messages"
              ref={messagesContainerRef}
              onScroll={handleScroll}
            >
              {activeRoomMessages.length === 0 && (
                <p className="empty-state center">No messages yet. Say hello!</p>
              )}

              {activeRoomMessages.map((msg, idx) => {
                const sender = users.find(u => idHex(u.identity) === idHex(msg.senderIdentity));
                const isMe = myIdentity && idHex(msg.senderIdentity) === idHex(myIdentity);
                const prevMsg = idx > 0 ? activeRoomMessages[idx - 1] : null;
                const isGrouped = prevMsg && idHex(prevMsg.senderIdentity) === idHex(msg.senderIdentity);
                const readers = getReadersForMessage(msg);
                const isEditing = editingMessageId === msg.id;
                const isEdited = msg.editedAt !== undefined && msg.editedAt !== null;
                const msgEdits = messageEdits
                  .filter((e: MessageEdit) => e.messageId === msg.id)
                  .sort((a: MessageEdit, b: MessageEdit) =>
                    a.editedAt.microsSinceUnixEpoch < b.editedAt.microsSinceUnixEpoch ? -1 : 1
                  );

                return (
                  <div key={String(msg.id)} className={`message-group ${isGrouped ? 'grouped' : ''} message-group-hoverable`}>
                    {!isGrouped && (
                      <div className="message-header">
                        <span
                          className="message-sender"
                          style={{ color: colorForName(sender?.name ?? '?') }}
                        >
                          {sender?.name ?? 'Unknown'}
                          {isMe ? ' (you)' : ''}
                        </span>
                        <span className="message-time">{formatTime(msg.sentAt)}</span>
                      </div>
                    )}
                    {isEditing ? (
                      <form onSubmit={handleSaveEdit} className="edit-form">
                        <textarea
                          className="input edit-textarea"
                          value={editText}
                          onChange={e => setEditText(e.target.value)}
                          rows={2}
                          autoFocus
                          onKeyDown={e => { if (e.key === 'Escape') cancelEditing(); }}
                        />
                        <div className="edit-actions">
                          <button type="button" className="btn btn-ghost btn-sm" onClick={cancelEditing}>Cancel</button>
                          <button type="submit" className="btn btn-primary btn-sm" disabled={!editText.trim()}>Save</button>
                        </div>
                      </form>
                    ) : (
                      <div className="message-content-row">
                        <span className="message-text">{msg.text}</span>
                        <div className="message-actions">
                          {isMe && (
                            <button className="edit-btn" onClick={() => startEditing(msg)}>Edit</button>
                          )}
                          <button className="reply-btn" onClick={() => handleOpenThread(msg.id)} title="Reply in thread">Reply</button>
                        </div>
                      </div>
                    )}
                    {isEdited && !isEditing && (
                      <button
                        className="edited-indicator"
                        onClick={() => setHistoryMessageId(historyMessageId === msg.id ? null : msg.id)}
                        title="View edit history"
                      >
                        (edited)
                      </button>
                    )}
                    {historyMessageId === msg.id && msgEdits.length > 0 && (
                      <div className="edit-history">
                        <div className="edit-history-title">Edit history</div>
                        {msgEdits.map((edit: MessageEdit, i: number) => (
                          <div key={String(edit.id)} className="edit-history-entry">
                            <span className="edit-history-version">v{i + 1}</span>
                            <span className="edit-history-time">{formatTime(edit.editedAt)}</span>
                            <span className="edit-history-text">{edit.previousText}</span>
                          </div>
                        ))}
                      </div>
                    )}
                    {msg.expiresAt !== undefined && msg.expiresAt !== null && (
                      <EphemeralCountdown expiresAtMicros={msg.expiresAt as bigint} />
                    )}
                    {readers.length > 0 && (
                      <div className="read-receipt">
                        Seen by {readers.join(', ')}
                      </div>
                    )}
                    <MessageReactions
                      messageId={msg.id}
                      reactions={messageReactions.filter(r => r.messageId === msg.id)}
                      myIdentity={myIdentity ?? null}
                      users={users}
                      onToggle={handleToggleReaction}
                    />
                    {(() => {
                      const replyCount = getReplyCount(msg.id);
                      if (replyCount === 0) return null;
                      const preview = getFirstReplyPreview(msg.id);
                      return (
                        <button
                          className={`thread-summary${threadParentId === msg.id ? ' active' : ''}`}
                          onClick={() => threadParentId === msg.id ? handleCloseThread() : handleOpenThread(msg.id)}
                        >
                          <span className="thread-count">{replyCount} {replyCount === 1 ? 'reply' : 'replies'}</span>
                          {preview && <span className="thread-preview">{preview}</span>}
                        </button>
                      );
                    })()}
                  </div>
                );
              })}

              {/* Typing indicator */}
              {activeTypers.length > 0 && (
                <div className="typing-indicator">
                  {activeTypers.length === 1
                    ? (() => {
                        const u = users.find(u => idHex(u.identity) === idHex(activeTypers[0].userIdentity));
                        return `${u?.name ?? 'Someone'} is typing…`;
                      })()
                    : 'Multiple users are typing…'}
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>

            {/* Scroll to bottom button */}
            {isScrolledUp && (
              <button
                className="scroll-to-bottom"
                onClick={() => {
                  setIsScrolledUp(false);
                  messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
                }}
              >
                ↓ New messages
              </button>
            )}

            {/* Pending scheduled messages */}
            {myPendingScheduled.length > 0 && (
              <div className="scheduled-panel">
                <div className="scheduled-panel-header">
                  Scheduled ({myPendingScheduled.length})
                </div>
                {myPendingScheduled.map(sm => (
                  <div key={String(sm.scheduledId)} className="scheduled-item">
                    <div className="scheduled-item-body">
                      <span className="scheduled-item-time">{formatScheduledAt(sm.scheduledAt)}</span>
                      <span className="scheduled-item-text">{sm.text}</span>
                    </div>
                    <button
                      className="btn btn-tiny btn-danger"
                      onClick={() => handleCancelScheduled(sm.scheduledId)}
                      title="Cancel scheduled message"
                    >
                      Cancel
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Message input */}
            <form className="message-input-bar" onSubmit={handleSendMessage}>
              <input
                className="input message-input"
                type="text"
                placeholder="Type a message…"
                value={messageText}
                onChange={handleMessageInput}
              />
              <select
                className="ephemeral-select"
                aria-label="Ephemeral duration"
                value={ephemeralDuration ?? ''}
                onChange={e => setEphemeralDuration(e.target.value ? Number(e.target.value) : null)}
                title="Disappearing message duration"
              >
                <option value="">Regular</option>
                <option value="30">Expire 30s</option>
                <option value="60">Expire 1m</option>
                <option value="300">Expire 5m</option>
                <option value="600">Expire 10m</option>
              </select>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => setShowScheduleModal(true)}
                title="Schedule a message"
              >
                Schedule
              </button>
              <button type="submit" className="btn btn-primary" disabled={!messageText.trim()}>
                Send
              </button>
            </form>
          </div>

          {/* Thread panel */}
          {threadParentId !== null && threadParentMsg && (
            <div className="thread-panel">
              <div className="thread-panel-header">
                <span className="thread-panel-title">Thread</span>
                <button className="btn btn-icon" onClick={handleCloseThread} title="Close thread">✕</button>
              </div>
              <div className="thread-parent-msg">
                {(() => {
                  const sender = users.find(u => idHex(u.identity) === idHex(threadParentMsg.senderIdentity));
                  return (
                    <>
                      <div className="message-header">
                        <span className="message-sender" style={{ color: colorForName(sender?.name ?? '?') }}>
                          {sender?.name ?? 'Unknown'}
                        </span>
                        <span className="message-time">{formatTime(threadParentMsg.sentAt)}</span>
                      </div>
                      <span className="message-text">{threadParentMsg.text}</span>
                    </>
                  );
                })()}
              </div>
              <div className="thread-replies">
                {threadMessages.length === 0 && (
                  <p className="empty-state center" style={{ padding: '16px' }}>No replies yet</p>
                )}
                {threadMessages.map(reply => {
                  const sender = users.find(u => idHex(u.identity) === idHex(reply.senderIdentity));
                  const isMe = myIdentity && idHex(reply.senderIdentity) === idHex(myIdentity);
                  return (
                    <div key={String(reply.id)} className="message-group">
                      <div className="message-header">
                        <span className="message-sender" style={{ color: colorForName(sender?.name ?? '?') }}>
                          {sender?.name ?? 'Unknown'}{isMe ? ' (you)' : ''}
                        </span>
                        <span className="message-time">{formatTime(reply.sentAt)}</span>
                      </div>
                      <span className="message-text">{reply.text}</span>
                    </div>
                  );
                })}
              </div>
              <form className="thread-input-bar" onSubmit={handleSendReply}>
                <input
                  className="input"
                  type="text"
                  placeholder="Reply in thread…"
                  value={threadReplyText}
                  onChange={e => setThreadReplyText(e.target.value)}
                  autoFocus
                />
                <button type="submit" className="btn btn-primary btn-sm" disabled={!threadReplyText.trim()}>
                  Reply
                </button>
              </form>
            </div>
          )}
          </div>
        )}
      </main>

      {showCreateRoom && (
        <CreateRoomModal
          onClose={() => setShowCreateRoom(false)}
          onCreate={handleCreateRoom}
        />
      )}

      {showScheduleModal && (
        <ScheduleMessageModal
          onClose={() => setShowScheduleModal(false)}
          onSchedule={handleScheduleMessage}
        />
      )}

      {showMembersPanel && activeRoomId !== null && (
        <MembersPanel
          roomId={activeRoomId}
          memberships={memberships}
          roomAdmins={roomAdmins}
          users={users}
          myIdentity={myIdentity ?? null}
          isAdmin={amAdminInActiveRoom}
          onKick={handleKickUser}
          onPromote={handlePromoteToAdmin}
          onClose={() => setShowMembersPanel(false)}
        />
      )}
    </div>
  );
}
