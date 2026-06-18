import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, Room, User, UserRoomRead, TypingIndicator, ScheduledMessage, MessageReaction, MessageEditHistory, RoomMember, RoomBan, RoomInvitation, MessageDraft } from './module_bindings/types';

// ---- helpers ----

function tsToMs(ts: { microsSinceUnixEpoch: bigint }): number {
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  return new Date(tsToMs(ts)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

// ---- Room activity helpers ----

const ACTIVITY_HOT_THRESHOLD = 5;    // messages in window → "Hot"
const ACTIVITY_ACTIVE_THRESHOLD = 1; // messages in window → "Active"
const ACTIVITY_HOT_WINDOW_MS = 2 * 60 * 1000;    // 2 min
const ACTIVITY_ACTIVE_WINDOW_MS = 5 * 60 * 1000;  // 5 min

type ActivityLevel = 'hot' | 'active' | 'none';

function getActivityLevel(roomId: bigint, allMessages: readonly { roomId: bigint; sentAt: { microsSinceUnixEpoch: bigint }; parentMessageId: bigint | undefined }[]): ActivityLevel {
  const now = Date.now();
  const roomRootMsgs = allMessages.filter(m => m.roomId === roomId && m.parentMessageId === undefined);
  const hotCount = roomRootMsgs.filter(m => (now - Number(m.sentAt.microsSinceUnixEpoch / 1000n)) < ACTIVITY_HOT_WINDOW_MS).length;
  if (hotCount >= ACTIVITY_HOT_THRESHOLD) return 'hot';
  const activeCount = roomRootMsgs.filter(m => (now - Number(m.sentAt.microsSinceUnixEpoch / 1000n)) < ACTIVITY_ACTIVE_WINDOW_MS).length;
  if (activeCount >= ACTIVITY_ACTIVE_THRESHOLD) return 'active';
  return 'none';
}

function ActivityBadge({ level }: { level: ActivityLevel }) {
  if (level === 'hot') return <span className="activity-badge activity-hot" title="Very active in the last 2 minutes">🔥 Hot</span>;
  if (level === 'active') return <span className="activity-badge activity-active" title="Active in the last 5 minutes">● Active</span>;
  return null;
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
const AUTO_AWAY_TIMEOUT_MS = 5 * 60 * 1000;

// ---- MessageList ----

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

interface MessageListProps {
  messages: readonly Message[];
  users: readonly User[];
  myIdentity: { toHexString(): string } | null | undefined;
  userRoomReads: readonly UserRoomRead[];
  reactions: readonly MessageReaction[];
  editHistory: readonly MessageEditHistory[];
  replyMap: Map<bigint, Message[]>;
  openThreadId: bigint | null;
  onToggleReaction: (messageId: bigint, emoji: string) => void;
  onEditMessage: (messageId: bigint, content: string) => void;
  onReply: (messageId: bigint) => void;
}

function MessageList({ messages, users, myIdentity, userRoomReads, reactions, editHistory, replyMap, openThreadId, onToggleReaction, onEditMessage, onReply }: MessageListProps) {
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
            const replies = replyMap.get(msg.id) ?? [];
            const replyCount = replies.length;
            const isThreadOpen = openThreadId === msg.id;
            return (
              <div
                key={String(msg.id)}
                className={`message-row${isEphemeral ? ' ephemeral-message' : ''}${isThreadOpen ? ' thread-active-msg' : ''}`}
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
                        <button
                          className="reaction-pick-btn reply-btn"
                          onClick={() => onReply(msg.id)}
                          title="Reply in thread"
                        >
                          ↩ Reply
                        </button>
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
                {/* Thread preview */}
                {replyCount > 0 && (
                  <div
                    className={`thread-preview${isThreadOpen ? ' thread-preview-active' : ''}`}
                    onClick={() => onReply(msg.id)}
                    role="button"
                    tabIndex={0}
                    onKeyDown={e => e.key === 'Enter' && onReply(msg.id)}
                  >
                    <span className="thread-preview-text">
                      {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
                    </span>
                    {replies[replies.length - 1] && (
                      <span className="thread-preview-snippet">
                        {getUserByIdentity(replies[replies.length - 1].sender.toHexString())?.name ?? 'Unknown'}:{' '}
                        {replies[replies.length - 1].content.slice(0, 60)}
                        {replies[replies.length - 1].content.length > 60 ? '…' : ''}
                      </span>
                    )}
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

// ---- ThreadPanel ----

interface ThreadPanelProps {
  parentMessage: Message;
  replies: Message[];
  users: readonly User[];
  myIdentity: { toHexString(): string } | null | undefined;
  reactions: readonly MessageReaction[];
  onClose: () => void;
  onSendReply: (content: string) => void;
  onToggleReaction: (messageId: bigint, emoji: string) => void;
}

function ThreadPanel({ parentMessage, replies, users, myIdentity, reactions, onClose, onSendReply, onToggleReaction }: ThreadPanelProps) {
  const [replyInput, setReplyInput] = useState('');
  const repliesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    repliesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [replies.length]);

  const getUserByIdentity = (hex: string): User | undefined =>
    users.find(u => u.identity.toHexString() === hex);

  const myHex = myIdentity?.toHexString();

  const getReactionGroups = (msgId: bigint): Map<string, MessageReaction[]> => {
    const grouped = new Map<string, MessageReaction[]>();
    for (const r of reactions.filter(r => r.messageId === msgId)) {
      if (!grouped.has(r.emoji)) grouped.set(r.emoji, []);
      grouped.get(r.emoji)!.push(r);
    }
    return grouped;
  };

  const sendReply = () => {
    const trimmed = replyInput.trim();
    if (!trimmed) return;
    onSendReply(trimmed);
    setReplyInput('');
  };

  const parentUser = getUserByIdentity(parentMessage.sender.toHexString());

  const renderMessage = (msg: Message, isParent = false) => {
    const user = getUserByIdentity(msg.sender.toHexString());
    const reactionGroups = getReactionGroups(msg.id);
    return (
      <div key={String(msg.id)} className={isParent ? 'thread-parent-msg' : 'thread-reply-msg'}>
        <div className="message-group-header">
          <div className="sender-avatar" style={{ background: nameColor(user?.name ?? '?') }}>
            {(user?.name?.[0] ?? '?').toUpperCase()}
          </div>
          <span className="sender-name" style={{ color: nameColor(user?.name ?? '?') }}>
            {user?.name ?? 'Unknown'}
            {user?.identity.toHexString() === myHex && <span className="you-label"> (you)</span>}
          </span>
          <span className="message-time">{formatTime(msg.sentAt)}</span>
        </div>
        <div className="thread-msg-body">
          <div className="message-content">{msg.content}</div>
          {reactionGroups.size > 0 && (
            <div className="reaction-row">
              {[...reactionGroups.entries()].map(([emoji, rs]) => {
                const iMine = rs.some(r => r.userIdentity.toHexString() === myHex);
                const names = rs.map(r => getUserByIdentity(r.userIdentity.toHexString())?.name ?? 'Unknown').join(', ');
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
        </div>
      </div>
    );
  };

  return (
    <div className="thread-panel">
      <div className="thread-panel-header">
        <span className="thread-panel-title">Thread</span>
        <button className="icon-btn" onClick={onClose} title="Close thread">✕</button>
      </div>

      <div className="thread-panel-body">
        {renderMessage(parentMessage, true)}
        <div className="thread-replies-divider">
          {replies.length === 0
            ? <span className="muted small">No replies yet</span>
            : <span className="muted small">{replies.length} {replies.length === 1 ? 'reply' : 'replies'}</span>
          }
        </div>
        {replies.map(reply => renderMessage(reply, false))}
        <div ref={repliesEndRef} />
      </div>

      <div className="thread-input-bar">
        <input
          type="text"
          className="message-input"
          placeholder={`Reply to ${parentUser?.name ?? 'thread'}...`}
          value={replyInput}
          onChange={e => setReplyInput(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendReply(); }
          }}
          maxLength={2000}
        />
        <button
          className="btn-primary"
          onClick={sendReply}
          disabled={!replyInput.trim()}
        >
          Reply
        </button>
      </div>
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
  const [isPrivateNewRoom, setIsPrivateNewRoom] = useState(false);
  const [showNewRoom, setShowNewRoom] = useState(false);
  const [roomError, setRoomError] = useState('');
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [typingActive, setTypingActive] = useState(false);
  const [, setTick] = useState(0);

  const [ephemeralTtl, setEphemeralTtl] = useState<number | null>(null);
  const [showMembersPanel, setShowMembersPanel] = useState(false);

  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleContent, setScheduleContent] = useState('');
  const [scheduleDateTime, setScheduleDateTime] = useState('');
  const [scheduleError, setScheduleError] = useState('');

  const [openThreadId, setOpenThreadId] = useState<bigint | null>(null);

  // Level 9: Private rooms & DMs
  const [showInviteModal, setShowInviteModal] = useState(false);
  const [inviteUsername, setInviteUsername] = useState('');
  const [inviteError, setInviteError] = useState('');
  const [pendingDmTarget, setPendingDmTarget] = useState<string | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const draftSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const selectedRoomIdRef = useRef<bigint | null>(null);
  selectedRoomIdRef.current = selectedRoomId;

  const autoAwaySetRef = useRef(false);
  const lastActivityRef = useRef(Date.now());
  const myUserRef = useRef<User | undefined>(undefined);
  const connRef = useRef<DbConnection | null>(null);
  const messageDraftsRef = useRef<readonly MessageDraft[]>([]);
  const myHexRef = useRef<string | undefined>(undefined);

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
        tables.roomMember
          .where(m => m.userIdentity.eq(myIdentity))
          .rightSemijoin(tables.message, (member, msg) => member.roomId.eq(msg.roomId)),
        tables.typingIndicator,
        tables.userRoomRead,
        tables.scheduledMessage,
        tables.messageReaction,
        tables.messageEditHistory,
        tables.roomInvitation.where(inv => inv.invitedIdentity.eq(myIdentity)),
        tables.messageDraft.where(d => d.userIdentity.eq(myIdentity)),
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
  const [roomInvitations] = useTable(tables.roomInvitation);
  const [messageDrafts] = useTable(tables.messageDraft);

  const myHex = myIdentity?.toHexString();
  const myUser = users.find(u => u.identity.toHexString() === myHex);

  myUserRef.current = myUser;
  connRef.current = conn;
  messageDraftsRef.current = messageDrafts;
  myHexRef.current = myHex;

  const myMemberships = roomMembers.filter(m => m.userIdentity.toHexString() === myHex);
  const myRoomIds = new Set(myMemberships.map(m => m.roomId));

  // Separate DMs and regular rooms
  const myRegularRooms = rooms
    .filter(r => myRoomIds.has(r.id) && !r.isDm)
    .sort((a, b) => {
      const d = a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  const myDmRooms = rooms
    .filter(r => myRoomIds.has(r.id) && r.isDm)
    .sort((a, b) => {
      const d = a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  // Other rooms: only public, non-DM rooms the user hasn't joined
  const otherRooms = rooms.filter(r => !myRoomIds.has(r.id) && !r.isPrivate && !r.isDm);

  const visibleUsers = users.filter(u => u.status !== 'offline' && u.status !== 'invisible' && u.name !== '');
  const onlineCount = users.filter(u => u.status === 'online' && u.name !== '').length;

  const selectedRoom = rooms.find(r => r.id === selectedRoomId);
  const roomMessages = messages
    .filter(m => m.roomId === selectedRoomId)
    .sort((a, b) => {
      const d = a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

  const rootRoomMessages = roomMessages.filter(m => m.parentMessageId === undefined);

  const replyMap = new Map<bigint, Message[]>();
  for (const m of roomMessages) {
    if (m.parentMessageId !== undefined) {
      const arr = replyMap.get(m.parentMessageId) ?? [];
      arr.push(m);
      replyMap.set(m.parentMessageId, arr);
    }
  }

  const openThreadParent = openThreadId !== null ? roomMessages.find(m => m.id === openThreadId) : undefined;
  const openThreadReplies = openThreadId !== null
    ? (replyMap.get(openThreadId) ?? []).sort((a, b) => {
        const d = a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch;
        return d > 0n ? 1 : d < 0n ? -1 : 0;
      })
    : [];

  const myPendingScheduled = scheduledMessages
    .filter(sm =>
      sm.roomId === selectedRoomId &&
      sm.sender.toHexString() === myHex
    )
    .sort((a, b) => {
      const d = a.sendAt - b.sendAt;
      return d > 0n ? 1 : d < 0n ? -1 : 0;
    });

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

  // My pending invitations
  const myPendingInvitations = roomInvitations.filter(
    inv => inv.invitedIdentity.toHexString() === myHex
  );

  const getUnreadCount = (roomId: bigint): number => {
    const read = userRoomReads.find(
      r => r.roomId === roomId && r.userIdentity.toHexString() === myHex
    );
    const lastReadId = read?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastReadId && m.parentMessageId === undefined).length;
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

  // Get DM display name (the other user's name)
  const getDmDisplayName = (room: Room): string => {
    const otherMember = roomMembers.find(
      m => m.roomId === room.id && m.userIdentity.toHexString() !== myHex
    );
    if (!otherMember) return 'DM';
    const otherUser = users.find(u => u.identity.toHexString() === otherMember.userIdentity.toHexString());
    return otherUser?.name ?? 'DM';
  };

  // Get room display name for header
  const getRoomDisplayName = (room: Room): string => {
    if (room.isDm) return getDmDisplayName(room);
    return room.name;
  };

  useEffect(() => {
    if (!conn || !selectedRoomId || rootRoomMessages.length === 0) return;
    const latest = rootRoomMessages[rootRoomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: latest.id });
  }, [selectedRoomId, rootRoomMessages.length]);

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  useEffect(() => {
    if (isAtBottom) scrollToBottom();
  }, [rootRoomMessages.length, isAtBottom, scrollToBottom]);

  useEffect(() => {
    setIsAtBottom(true);
    setTimeout(() => messagesEndRef.current?.scrollIntoView(), 50);
    const draft = selectedRoomId !== null
      ? messageDraftsRef.current.find(
          d => d.roomId === selectedRoomId && d.userIdentity.toHexString() === myHexRef.current
        )
      : undefined;
    setMessageInput(draft?.content ?? '');
    setShowMembersPanel(false);
    setOpenThreadId(null);
  }, [selectedRoomId]);

  // Watch for DM room being created (pendingDmTarget)
  useEffect(() => {
    if (!pendingDmTarget) return;
    const dmRoom = rooms.find(r => {
      if (!r.isDm) return false;
      const members = roomMembers.filter(m => m.roomId === r.id);
      return members.some(m => m.userIdentity.toHexString() === myHex)
        && members.some(m => m.userIdentity.toHexString() === pendingDmTarget);
    });
    if (dmRoom) {
      setSelectedRoomId(dmRoom.id);
      setPendingDmTarget(null);
    }
  }, [rooms, roomMembers, pendingDmTarget, myHex]);

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

    // Debounced draft save
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    const capturedConn = conn;
    const capturedRoomId = selectedRoomId;
    draftSaveTimerRef.current = setTimeout(() => {
      capturedConn.reducers.saveDraft({ roomId: capturedRoomId, content: value });
    }, 800);
  };

  useEffect(() => {
    return () => {
      if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
      stopTyping();
    };
  }, [selectedRoomId, stopTyping]);

  useEffect(() => {
    const onActivity = () => {
      lastActivityRef.current = Date.now();
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
      parentMessageId: undefined,
    });
    conn.reducers.saveDraft({ roomId: selectedRoomId, content: '' });
    if (draftSaveTimerRef.current) clearTimeout(draftSaveTimerRef.current);
    setMessageInput('');
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    stopTyping();
    setIsAtBottom(true);
  };

  const handleSendReply = (content: string) => {
    if (!conn || !selectedRoomId || !openThreadId) return;
    conn.reducers.sendMessage({
      roomId: selectedRoomId,
      content,
      ttlSeconds: undefined,
      parentMessageId: openThreadId,
    });
  };

  const handleSetName = () => {
    if (!conn || !nameInput.trim()) { setNameError('Please enter a display name'); return; }
    conn.reducers.setName({ name: nameInput.trim() });
    setNameError('');
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) { setRoomError('Please enter a room name'); return; }
    conn.reducers.createRoom({ name: newRoomName.trim(), isPrivate: isPrivateNewRoom });
    setNewRoomName('');
    setIsPrivateNewRoom(false);
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
    autoAwaySetRef.current = false;
    conn.reducers.setStatus({ status });
  };

  const handleCreateDm = (targetUser: User) => {
    if (!conn) return;
    // Check if DM already exists
    const existingDm = rooms.find(r => {
      if (!r.isDm) return false;
      const members = roomMembers.filter(m => m.roomId === r.id);
      return members.some(m => m.userIdentity.toHexString() === myHex)
        && members.some(m => m.userIdentity.toHexString() === targetUser.identity.toHexString());
    });
    if (existingDm) {
      setSelectedRoomId(existingDm.id);
      return;
    }
    setPendingDmTarget(targetUser.identity.toHexString());
    conn.reducers.createDm({ targetIdentity: targetUser.identity });
  };

  const handleInviteUser = () => {
    if (!conn || !selectedRoomId) return;
    if (!inviteUsername.trim()) { setInviteError('Enter a username'); return; }
    conn.reducers.inviteUser({ roomId: selectedRoomId, username: inviteUsername.trim() });
    setShowInviteModal(false);
    setInviteUsername('');
    setInviteError('');
  };

  const handleAcceptInvitation = (invitationId: bigint) => {
    if (!conn) return;
    conn.reducers.acceptInvitation({ invitationId });
  };

  const handleDeclineInvitation = (invitationId: bigint) => {
    if (!conn) return;
    conn.reducers.declineInvitation({ invitationId });
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

        {/* Pending invitations */}
        {myPendingInvitations.length > 0 && (
          <div className="sidebar-section invitations-section">
            <div className="section-header">
              <span>Invitations ({myPendingInvitations.length})</span>
            </div>
            {myPendingInvitations.map(inv => {
              const invRoom = rooms.find(r => r.id === inv.roomId);
              const inviter = users.find(u => u.identity.toHexString() === inv.inviterIdentity.toHexString());
              return (
                <div key={String(inv.id)} className="invitation-item">
                  <div className="invitation-info">
                    <span className="invitation-room-name">
                      {invRoom ? invRoom.name : 'Unknown room'}
                    </span>
                    <span className="muted small">from {inviter?.name ?? 'Unknown'}</span>
                  </div>
                  <div className="invitation-actions">
                    <button
                      className="btn-primary btn-xs"
                      onClick={() => handleAcceptInvitation(inv.id)}
                    >
                      Accept
                    </button>
                    <button
                      className="btn-ghost btn-xs"
                      onClick={() => handleDeclineInvitation(inv.id)}
                    >
                      Decline
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowNewRoom(true)} title="Create room">
              +
            </button>
          </div>

          {myRegularRooms.length === 0 && (
            <p className="empty-hint">No rooms yet — create one!</p>
          )}

          {myRegularRooms.map(room => {
            const unread = getUnreadCount(room.id);
            const activity = getActivityLevel(room.id, messages);
            return (
              <button
                key={String(room.id)}
                className={`room-btn ${selectedRoomId === room.id ? 'active' : ''}`}
                onClick={() => setSelectedRoomId(room.id)}
              >
                <span className="room-hash">{room.isPrivate ? '🔒' : '#'}</span>
                <span className="room-btn-name">{room.name}</span>
                {room.isPrivate && <span className="private-label muted small">private</span>}
                <ActivityBadge level={activity} />
                {unread > 0 && <span className="badge">{unread}</span>}
              </button>
            );
          })}

          {otherRooms.length > 0 && (
            <>
              <div className="subsection-label">Other Rooms</div>
              {otherRooms.map(room => {
                const activity = getActivityLevel(room.id, messages);
                return (
                  <div key={String(room.id)} className="room-btn other-room">
                    <span className="room-hash">#</span>
                    <span className="room-btn-name">{room.name}</span>
                    <ActivityBadge level={activity} />
                    <button className="join-btn" onClick={() => handleJoinRoom(room.id)}>
                      Join
                    </button>
                  </div>
                );
              })}
            </>
          )}
        </div>

        {/* DMs section */}
        {myDmRooms.length > 0 && (
          <div className="sidebar-section">
            <div className="section-header">
              <span>Direct Messages</span>
            </div>
            {myDmRooms.map(room => {
              const unread = getUnreadCount(room.id);
              const displayName = getDmDisplayName(room);
              const activity = getActivityLevel(room.id, messages);
              return (
                <button
                  key={String(room.id)}
                  className={`room-btn ${selectedRoomId === room.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoomId(room.id)}
                >
                  <span className="room-hash">💬</span>
                  <span className="room-btn-name">{displayName}</span>
                  <ActivityBadge level={activity} />
                  {unread > 0 && <span className="badge">{unread}</span>}
                </button>
              );
            })}
          </div>
        )}

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
              {u.identity.toHexString() !== myHex && (
                <button
                  className="dm-btn btn-xs"
                  onClick={() => handleCreateDm(u)}
                  title={`Send DM to ${u.name}`}
                >
                  DM
                </button>
              )}
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
                <span className="room-hash-lg">{selectedRoom.isDm ? '💬' : (selectedRoom.isPrivate ? '🔒' : '#')}</span>
                <h2 className="chat-room-title">{getRoomDisplayName(selectedRoom)}</h2>
              </div>
              <div className="chat-header-actions">
                {!selectedRoom.isDm && iAmAdminInSelected && selectedRoom.isPrivate && (
                  <button
                    className="btn-ghost"
                    onClick={() => { setShowInviteModal(true); setInviteError(''); setInviteUsername(''); }}
                  >
                    Invite
                  </button>
                )}
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
                      <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                        {iAmAdminInSelected && selectedRoom.isPrivate && !selectedRoom.isDm && (
                          <button
                            className="btn-ghost btn-sm"
                            onClick={() => { setShowInviteModal(true); setInviteError(''); setInviteUsername(''); }}
                          >
                            Invite
                          </button>
                        )}
                        <button className="icon-btn" onClick={() => setShowMembersPanel(false)}>✕</button>
                      </div>
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

                {/* Messages + Thread Panel row */}
                <div className="messages-thread-row">
                  <div
                    ref={messagesContainerRef}
                    className="messages-area"
                    onScroll={handleScroll}
                  >
                    {rootRoomMessages.length === 0 ? (
                      <div className="fullscreen-center flex-1">
                        <p className="muted">No messages yet — say something!</p>
                      </div>
                    ) : (
                      <MessageList
                        messages={rootRoomMessages}
                        users={users}
                        myIdentity={myIdentity}
                        userRoomReads={userRoomReads}
                        reactions={messageReactions}
                        editHistory={messageEditHistories}
                        replyMap={replyMap}
                        openThreadId={openThreadId}
                        onToggleReaction={handleToggleReaction}
                        onEditMessage={handleEditMessage}
                        onReply={setOpenThreadId}
                      />
                    )}
                    <div ref={messagesEndRef} />
                  </div>

                  {openThreadId !== null && openThreadParent && (
                    <ThreadPanel
                      parentMessage={openThreadParent}
                      replies={openThreadReplies}
                      users={users}
                      myIdentity={myIdentity}
                      reactions={messageReactions}
                      onClose={() => setOpenThreadId(null)}
                      onSendReply={handleSendReply}
                      onToggleReaction={handleToggleReaction}
                    />
                  )}
                </div>

                {!isAtBottom && (
                  <button
                    className="scroll-btn"
                    onClick={() => { scrollToBottom(); setIsAtBottom(true); }}
                  >
                    ↓ Scroll to latest
                  </button>
                )}

                <ScheduledMessagesList
                  pending={myPendingScheduled}
                  onCancel={handleCancelScheduled}
                />

                <div className="typing-row">
                  {typingUsers.length > 0 && (
                    <span className="typing-text">
                      {typingUsers.length === 1
                        ? `${typingUsers[0].name} is typing...`
                        : `${typingUsers.map(u => u.name).join(', ')} are typing...`}
                    </span>
                  )}
                </div>

                <div className="input-bar">
                  <input
                    type="text"
                    className="message-input"
                    placeholder={`Message ${selectedRoom.isDm ? getDmDisplayName(selectedRoom) : '#' + selectedRoom.name}`}
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
          onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); setIsPrivateNewRoom(false); }}
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
                if (e.key === 'Escape') { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); setIsPrivateNewRoom(false); }
              }}
              maxLength={64}
              autoFocus
            />
            <label className="private-checkbox-label">
              <input
                type="checkbox"
                checked={isPrivateNewRoom}
                onChange={e => setIsPrivateNewRoom(e.target.checked)}
              />
              <span>Private (invite-only)</span>
            </label>
            {roomError && <p className="error-msg">{roomError}</p>}
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => { setShowNewRoom(false); setRoomError(''); setNewRoomName(''); setIsPrivateNewRoom(false); }}
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

      {/* Invite User Modal */}
      {showInviteModal && (
        <div
          className="modal-backdrop"
          onClick={() => { setShowInviteModal(false); setInviteError(''); setInviteUsername(''); }}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Invite to {selectedRoom ? selectedRoom.name : 'Room'}</h3>
            <input
              type="text"
              placeholder="Enter username..."
              value={inviteUsername}
              onChange={e => setInviteUsername(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') handleInviteUser();
                if (e.key === 'Escape') { setShowInviteModal(false); setInviteError(''); setInviteUsername(''); }
              }}
              maxLength={32}
              autoFocus
            />
            {inviteError && <p className="error-msg">{inviteError}</p>}
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => { setShowInviteModal(false); setInviteError(''); setInviteUsername(''); }}
              >
                Cancel
              </button>
              <button
                className="btn-primary"
                onClick={handleInviteUser}
                disabled={!inviteUsername.trim()}
              >
                Invite
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
