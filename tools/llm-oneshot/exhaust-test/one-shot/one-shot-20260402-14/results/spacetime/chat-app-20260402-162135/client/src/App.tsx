import { useState, useEffect, useRef, useCallback } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';
import type {
  Message,
  User,
  Room,
  RoomMember,
  TypingIndicator,
  ReadReceipt,
  RoomLastRead,
  MessageReaction,
  ScheduledMessage,
  MessageEdit,
} from './module_bindings/types';

// ============ HELPERS ============

function tsToDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

function formatTime(ts: { microsSinceUnixEpoch: bigint }): string {
  return tsToDate(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatRelative(ts: { microsSinceUnixEpoch: bigint }): string {
  const now = Date.now();
  const diff = now - Number(ts.microsSinceUnixEpoch / 1000n);
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return tsToDate(ts).toLocaleDateString();
}

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

const STATUS_ICONS: Record<string, string> = {
  online: '🟢',
  away: '🟡',
  dnd: '🔴',
  invisible: '⚫',
};

// ============ MAIN APP ============

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Table subscriptions (useTable handles subscribes internally)
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.room_member);
  const [messages] = useTable(tables.message);
  const [messageEdits] = useTable(tables.message_edit);
  const [typingIndicators] = useTable(tables.typing_indicator);
  const [readReceipts] = useTable(tables.read_receipt);
  const [roomLastReads] = useTable(tables.room_last_read);
  const [messageReactions] = useTable(tables.message_reaction);
  const [scheduledMessages] = useTable(tables.scheduled_message);

  // UI state
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [registerName, setRegisterName] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [showHistory, setShowHistory] = useState<bigint | null>(null);
  const [showScheduledPanel, setShowScheduledPanel] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activityTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Derived data
  const me = myIdentity ? users.find(u => u.identity.toHexString() === myIdentity.toHexString()) : null;
  const isRegistered = !!me;

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, selectedRoomId]);

  // Activity timer: update activity every 60s to prevent auto-away
  useEffect(() => {
    if (!conn || !isActive || !isRegistered) return;
    activityTimerRef.current = setInterval(() => {
      conn.reducers.updateActivity({});
    }, 60_000);
    return () => {
      if (activityTimerRef.current) clearInterval(activityTimerRef.current);
    };
  }, [conn, isActive, isRegistered]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!conn || !isActive || !selectedRoomId || !isRegistered) return;
    const roomMessages = messages.filter(m => m.roomId === selectedRoomId && !m.deleted);
    if (roomMessages.length === 0) return;
    const lastMsg = roomMessages.reduce((a, b) => a.id > b.id ? a : b);
    conn.reducers.markRoomRead({ roomId: selectedRoomId, lastMessageId: lastMsg.id });
  }, [selectedRoomId, messages.length, conn, isActive, isRegistered]);

  // Handle register
  const handleRegister = useCallback(() => {
    if (!conn || !registerName.trim()) return;
    conn.reducers.register({ name: registerName.trim() });
  }, [conn, registerName]);

  // Handle send message
  const handleSend = useCallback(() => {
    if (!conn || !messageText.trim() || !selectedRoomId) return;
    if (isEphemeral) {
      conn.reducers.sendEphemeralMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        durationSeconds: ephemeralDuration,
      });
    } else if (showSchedule && scheduleTime) {
      const dt = new Date(scheduleTime);
      const micros = BigInt(dt.getTime()) * 1000n;
      conn.reducers.scheduleMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        sendAtMicros: micros,
      });
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    }
    setMessageText('');
    setIsEphemeral(false);
    setShowSchedule(false);
    setScheduleTime('');
    // Clear typing indicator
    conn.reducers.clearTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
  }, [conn, messageText, selectedRoomId, isEphemeral, ephemeralDuration, showSchedule, scheduleTime]);

  // Handle typing
  const handleTyping = useCallback((value: string) => {
    setMessageText(value);
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.clearTyping({ roomId: selectedRoomId });
    }, 3000);
  }, [conn, selectedRoomId]);

  // Get room messages
  const roomMessages = selectedRoomId
    ? messages
        .filter(m => m.roomId === selectedRoomId)
        .sort((a, b) => (a.id < b.id ? -1 : 1))
    : [];

  // Get my membership in selected room
  const myMembership = selectedRoomId && myIdentity
    ? roomMembers.find(m => m.roomId === selectedRoomId && m.identity.toHexString() === myIdentity.toHexString())
    : null;

  const isMember = myMembership && !myMembership.banned;
  const isAdmin = myMembership?.role === 'admin';

  // Unread count per room
  const getUnreadCount = (roomId: bigint): number => {
    const myRead = myIdentity
      ? roomLastReads.find(lr => lr.roomId === roomId && lr.identity.toHexString() === myIdentity.toHexString())
      : null;
    const lastReadId = myRead?.lastReadMessageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && !m.deleted && m.id > lastReadId && !m.sender.equals(myIdentity!)).length;
  };

  // Typing users in selected room
  const typingUsers = selectedRoomId
    ? typingIndicators
        .filter(ti => ti.roomId === selectedRoomId && myIdentity && !ti.identity.equals(myIdentity))
        .map(ti => users.find(u => u.identity.toHexString() === ti.identity.toHexString())?.name ?? 'Someone')
    : [];

  // Members of selected room
  const roomMembersList = selectedRoomId
    ? roomMembers.filter(m => m.roomId === selectedRoomId)
    : [];

  // My scheduled messages in selected room
  const myScheduledMessages = selectedRoomId && myIdentity
    ? scheduledMessages.filter(sm => sm.roomId === selectedRoomId && sm.senderIdentity.toHexString() === myIdentity.toHexString())
    : [];

  if (!isActive) {
    return <div className="loading">Connecting to SpacetimeDB...</div>;
  }

  if (!isRegistered) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <h1>SpacetimeDB Chat</h1>
          <p>Choose a display name to get started</p>
          <input
            type="text"
            value={registerName}
            onChange={e => setRegisterName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            placeholder="Your name..."
            maxLength={32}
            autoFocus
          />
          <button onClick={handleRegister} disabled={!registerName.trim()}>
            Join
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="app">
      {/* SIDEBAR */}
      <aside className="sidebar">
        {/* User info + status */}
        <div className="sidebar-header">
          <div className="user-info">
            <span className="user-name">{me?.name}</span>
            <select
              className="status-select"
              value={me?.status ?? 'online'}
              onChange={e => conn?.reducers.updateStatus({ status: e.target.value })}
            >
              <option value="online">🟢 Online</option>
              <option value="away">🟡 Away</option>
              <option value="dnd">🔴 Do Not Disturb</option>
              <option value="invisible">⚫ Invisible</option>
            </select>
          </div>
          <h2>SpacetimeDB Chat</h2>
        </div>

        {/* Room list */}
        <div className="sidebar-section">
          <div className="section-header">
            <span>Rooms</span>
            <button className="icon-btn" onClick={() => setShowCreateRoom(v => !v)} title="Create Room">+</button>
          </div>

          {showCreateRoom && (
            <div className="create-room-form">
              <input
                type="text"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => {
                  if (e.key === 'Enter' && newRoomName.trim() && conn) {
                    conn.reducers.createRoom({ name: newRoomName.trim() });
                    setNewRoomName('');
                    setShowCreateRoom(false);
                  }
                }}
                placeholder="Room name..."
                autoFocus
              />
            </div>
          )}

          <div className="room-list">
            {rooms.map(r => {
              const unread = myIdentity ? getUnreadCount(r.id) : 0;
              const isMemberOfRoom = roomMembers.some(m => m.roomId === r.id && myIdentity && m.identity.toHexString() === myIdentity.toHexString() && !m.banned);
              return (
                <div
                  key={r.id.toString()}
                  className={`room-item ${selectedRoomId === r.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoomId(r.id)}
                >
                  <span className="room-name">#{r.name}</span>
                  <div className="room-badges">
                    {unread > 0 && <span className="badge">{unread}</span>}
                    {!isMemberOfRoom && (
                      <button
                        className="join-btn"
                        onClick={e => {
                          e.stopPropagation();
                          conn?.reducers.joinRoom({ roomId: r.id });
                          setSelectedRoomId(r.id);
                        }}
                      >
                        Join
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
            {rooms.length === 0 && (
              <div className="empty-hint">No rooms yet. Create one!</div>
            )}
          </div>
        </div>

        {/* Online users */}
        <div className="sidebar-section users-section">
          <div className="section-header">
            <span>Users ({users.filter(u => u.status !== 'invisible').length})</span>
          </div>
          <div className="user-list">
            {users
              .filter(u => u.status !== 'invisible' || (myIdentity && u.identity.toHexString() === myIdentity.toHexString()))
              .sort((a, b) => {
                const order: Record<string, number> = { online: 0, dnd: 1, away: 2, invisible: 3 };
                return (order[a.status] ?? 9) - (order[b.status] ?? 9);
              })
              .map(u => (
                <div key={u.identity.toHexString()} className="user-item">
                  <span className="status-icon">{STATUS_ICONS[u.status] ?? '⚫'}</span>
                  <span className="user-name">{u.name}{myIdentity && u.identity.toHexString() === myIdentity.toHexString() ? ' (you)' : ''}</span>
                  {u.status === 'away' && (
                    <span className="last-active">{formatRelative(u.lastActive)}</span>
                  )}
                </div>
              ))}
          </div>
        </div>
      </aside>

      {/* MAIN CHAT AREA */}
      <main className="chat-main">
        {!selectedRoomId ? (
          <div className="no-room">
            <p>Select a room to start chatting</p>
          </div>
        ) : (
          <>
            {/* Chat header */}
            <header className="chat-header">
              <div className="chat-header-left">
                <h3>#{rooms.find(r => r.id === selectedRoomId)?.name}</h3>
                {isMember && (
                  <button
                    className="leave-btn"
                    onClick={() => {
                      conn?.reducers.leaveRoom({ roomId: selectedRoomId });
                      setSelectedRoomId(null);
                    }}
                  >
                    Leave
                  </button>
                )}
              </div>
              <div className="chat-header-right">
                {myScheduledMessages.length > 0 && (
                  <button
                    className="scheduled-btn"
                    onClick={() => setShowScheduledPanel(v => !v)}
                    title="Scheduled messages"
                  >
                    ⏰ {myScheduledMessages.length}
                  </button>
                )}
                {/* Room members */}
                <div className="member-avatars">
                  {roomMembersList.filter(m => !m.banned).slice(0, 5).map(m => {
                    const u = users.find(u => u.identity.toHexString() === m.identity.toHexString());
                    return (
                      <span key={m.identity.toHexString()} className="member-avatar" title={`${u?.name ?? '?'} (${m.role})`}>
                        {STATUS_ICONS[u?.status ?? 'online']}
                      </span>
                    );
                  })}
                </div>
              </div>
            </header>

            {/* Scheduled messages panel */}
            {showScheduledPanel && (
              <div className="scheduled-panel">
                <h4>Scheduled Messages</h4>
                {myScheduledMessages.map(sm => (
                  <div key={sm.scheduledId.toString()} className="scheduled-item">
                    <span className="scheduled-text">{sm.text}</span>
                    <button
                      className="cancel-btn"
                      onClick={() => conn?.reducers.cancelScheduledMessage({ scheduledId: sm.scheduledId })}
                    >
                      Cancel
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Messages */}
            <div className="messages-area">
              {!isMember && (
                <div className="not-member-banner">
                  <span>You are not a member of this room.</span>
                  <button onClick={() => conn?.reducers.joinRoom({ roomId: selectedRoomId! })}>
                    Join Room
                  </button>
                </div>
              )}
              {roomMessages.map(msg => (
                <MessageItem
                  key={msg.id.toString()}
                  msg={msg}
                  users={users}
                  myIdentity={myIdentity}
                  readReceipts={readReceipts.filter(r => r.messageId === msg.id)}
                  reactions={messageReactions.filter(r => r.messageId === msg.id)}
                  edits={messageEdits.filter(e => e.messageId === msg.id)}
                  showHistory={showHistory === msg.id}
                  isAdmin={!!isAdmin}
                  onToggleHistory={() => setShowHistory(prev => prev === msg.id ? null : msg.id)}
                  onMarkRead={() => conn?.reducers.markMessageRead({ messageId: msg.id })}
                  onReact={(emoji) => conn?.reducers.toggleReaction({ messageId: msg.id, emoji })}
                  onEdit={() => { setEditingMessageId(msg.id); setEditText(msg.text); }}
                  onDelete={() => conn?.reducers.deleteMessage({ messageId: msg.id })}
                  onKick={(identity) => conn?.reducers.kickUser({ roomId: selectedRoomId!, targetIdentity: identity })}
                  onPromote={(identity) => conn?.reducers.promoteUser({ roomId: selectedRoomId!, targetIdentity: identity })}
                  roomMembers={roomMembersList}
                />
              ))}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicators */}
            {typingUsers.length > 0 && (
              <div className="typing-indicator">
                {typingUsers.length === 1
                  ? `${typingUsers[0]} is typing...`
                  : `Multiple users are typing...`}
              </div>
            )}

            {/* Edit mode */}
            {editingMessageId && (
              <div className="edit-bar">
                <span>Editing message:</span>
                <input
                  type="text"
                  value={editText}
                  onChange={e => setEditText(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter') {
                      conn?.reducers.editMessage({ messageId: editingMessageId, newText: editText });
                      setEditingMessageId(null);
                      setEditText('');
                    }
                    if (e.key === 'Escape') {
                      setEditingMessageId(null);
                      setEditText('');
                    }
                  }}
                  autoFocus
                />
                <button onClick={() => {
                  conn?.reducers.editMessage({ messageId: editingMessageId, newText: editText });
                  setEditingMessageId(null);
                  setEditText('');
                }}>Save</button>
                <button onClick={() => { setEditingMessageId(null); setEditText(''); }}>Cancel</button>
              </div>
            )}

            {/* Input area */}
            {isMember && !editingMessageId && (
              <div className="input-area">
                <div className="input-options">
                  <label className="option-toggle">
                    <input
                      type="checkbox"
                      checked={isEphemeral}
                      onChange={e => { setIsEphemeral(e.target.checked); setShowSchedule(false); }}
                    />
                    Ephemeral
                  </label>
                  {isEphemeral && (
                    <select
                      value={ephemeralDuration}
                      onChange={e => setEphemeralDuration(Number(e.target.value))}
                    >
                      <option value={30}>30s</option>
                      <option value={60}>1m</option>
                      <option value={300}>5m</option>
                      <option value={3600}>1h</option>
                    </select>
                  )}
                  <label className="option-toggle">
                    <input
                      type="checkbox"
                      checked={showSchedule}
                      onChange={e => { setShowSchedule(e.target.checked); setIsEphemeral(false); }}
                    />
                    Schedule
                  </label>
                  {showSchedule && (
                    <input
                      type="datetime-local"
                      value={scheduleTime}
                      onChange={e => setScheduleTime(e.target.value)}
                    />
                  )}
                </div>
                <div className="input-row">
                  <input
                    type="text"
                    className="message-input"
                    value={messageText}
                    onChange={e => handleTyping(e.target.value)}
                    onKeyDown={e => e.key === 'Enter' && !e.shiftKey && handleSend()}
                    placeholder={
                      isEphemeral
                        ? `Send ephemeral message (${ephemeralDuration}s)...`
                        : showSchedule
                        ? 'Type a message to schedule...'
                        : 'Type a message...'
                    }
                  />
                  <button
                    className="send-btn"
                    onClick={handleSend}
                    disabled={!messageText.trim() || (showSchedule && !scheduleTime)}
                  >
                    {showSchedule ? '⏰' : '➤'}
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </main>

      {/* ADMIN PANEL (right sidebar when admin) */}
      {!!isAdmin && selectedRoomId !== null && (
        <aside className="admin-panel">
          <h3>Room Members</h3>
          {roomMembersList.map(m => {
            const u = users.find(u => u.identity.toHexString() === m.identity.toHexString());
            const isMe = myIdentity && m.identity.toHexString() === myIdentity.toHexString();
            return (
              <div key={m.identity.toHexString()} className={`admin-member ${m.banned ? 'banned' : ''}`}>
                <span>{u?.name ?? '?'} {m.role === 'admin' ? '(admin)' : ''} {m.banned ? '(banned)' : ''}</span>
                {!isMe && !m.banned && (
                  <div className="admin-actions">
                    {m.role !== 'admin' && (
                      <button
                        onClick={() => conn?.reducers.promoteUser({ roomId: selectedRoomId, targetIdentity: m.identity })}
                        title="Promote to admin"
                      >↑</button>
                    )}
                    <button
                      onClick={() => conn?.reducers.kickUser({ roomId: selectedRoomId, targetIdentity: m.identity })}
                      title="Kick user"
                    >✕</button>
                  </div>
                )}
                {!isMe && m.banned && (
                  <button onClick={() => conn?.reducers.unbanUser({ roomId: selectedRoomId, targetIdentity: m.identity })}>
                    Unban
                  </button>
                )}
              </div>
            );
          })}
        </aside>
      )}
    </div>
  );
}

// ============ MESSAGE ITEM COMPONENT ============

interface MessageItemProps {
  msg: Message;
  users: readonly User[];
  myIdentity: Identity | undefined;
  readReceipts: readonly ReadReceipt[];
  reactions: readonly MessageReaction[];
  edits: readonly MessageEdit[];
  showHistory: boolean;
  isAdmin: boolean;
  onToggleHistory: () => void;
  onMarkRead: () => void;
  onReact: (emoji: string) => void;
  onEdit: () => void;
  onDelete: () => void;
  onKick: (identity: Identity) => void;
  onPromote: (identity: Identity) => void;
  roomMembers: readonly RoomMember[];
}

function MessageItem({
  msg,
  users,
  myIdentity,
  readReceipts,
  reactions,
  edits,
  showHistory,
  isAdmin,
  onToggleHistory,
  onMarkRead,
  onReact,
  onEdit,
  onDelete,
}: MessageItemProps) {
  const sender = users.find(u => u.identity.toHexString() === msg.sender.toHexString());
  const isMe = myIdentity && msg.sender.toHexString() === myIdentity.toHexString();
  const [showActions, setShowActions] = useState(false);
  const [showReactPicker, setShowReactPicker] = useState(false);

  // Count reactions by emoji
  const reactionGroups = EMOJIS.map(emoji => ({
    emoji,
    count: reactions.filter(r => r.emoji === emoji).length,
    mine: myIdentity ? reactions.some(r => r.emoji === emoji && r.reactor.toHexString() === myIdentity.toHexString()) : false,
    users: reactions.filter(r => r.emoji === emoji).map(r => users.find(u => u.identity.toHexString() === r.reactor.toHexString())?.name ?? '?'),
  })).filter(g => g.count > 0);

  // Seen by
  const seenByUsers = readReceipts
    .filter(r => !myIdentity || r.reader.toHexString() !== myIdentity.toHexString())
    .map(r => users.find(u => u.identity.toHexString() === r.reader.toHexString())?.name ?? '?');

  // Ephemeral countdown
  const [expiresIn, setExpiresIn] = useState<number | null>(null);
  useEffect(() => {
    if (!msg.isEphemeral || !msg.expiresAt || msg.deleted) return;
    const update = () => {
      const secs = Math.max(0, Math.floor((Number(msg.expiresAt!.microsSinceUnixEpoch / 1000n) - Date.now()) / 1000));
      setExpiresIn(secs);
    };
    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [msg.isEphemeral, msg.expiresAt, msg.deleted]);

  if (msg.deleted && !isAdmin) {
    return (
      <div className="message deleted">
        <span className="deleted-text">[message deleted]</span>
      </div>
    );
  }

  return (
    <div
      className={`message ${isMe ? 'mine' : ''} ${msg.isEphemeral ? 'ephemeral' : ''}`}
      onMouseEnter={() => { setShowActions(true); onMarkRead(); }}
      onMouseLeave={() => { setShowActions(false); setShowReactPicker(false); }}
    >
      <div className="message-header">
        <span className="sender-name">{sender?.name ?? '?'}</span>
        <span className="message-time">{formatTime(msg.sentAt)}</span>
        {msg.editedAt && (
          <button className="edited-indicator" onClick={onToggleHistory} title="View edit history">
            (edited)
          </button>
        )}
        {msg.isEphemeral && expiresIn !== null && (
          <span className="ephemeral-badge" title="Ephemeral message">
            ⏱ {expiresIn}s
          </span>
        )}
      </div>

      <div className="message-body">
        <p className={msg.deleted ? 'deleted-text' : ''}>{msg.text}</p>
      </div>

      {/* Edit history */}
      {showHistory && edits.length > 0 && (
        <div className="edit-history">
          <strong>Edit history:</strong>
          {[...edits]
            .sort((a, b) => (a.id < b.id ? -1 : 1))
            .map(e => (
              <div key={e.id.toString()} className="edit-item">
                <span className="edit-time">{formatTime(e.editedAt)}</span>
                <span className="edit-text">{e.oldText}</span>
              </div>
            ))}
        </div>
      )}

      {/* Reactions */}
      {reactionGroups.length > 0 && (
        <div className="reactions">
          {reactionGroups.map(g => (
            <button
              key={g.emoji}
              className={`reaction ${g.mine ? 'mine' : ''}`}
              onClick={() => onReact(g.emoji)}
              title={`Reacted by: ${g.users.join(', ')}`}
            >
              {g.emoji} {g.count}
            </button>
          ))}
        </div>
      )}

      {/* Read receipts */}
      {seenByUsers.length > 0 && isMe && (
        <div className="seen-by">
          Seen by: {seenByUsers.join(', ')}
        </div>
      )}

      {/* Action buttons */}
      {showActions && !msg.deleted && (
        <div className="message-actions">
          <button onClick={() => setShowReactPicker(v => !v)} title="React">😊</button>
          {isMe && !msg.isEphemeral && <button onClick={onEdit} title="Edit">✏️</button>}
          {(isMe || isAdmin) && <button onClick={onDelete} title="Delete">🗑️</button>}
        </div>
      )}

      {/* Emoji picker */}
      {showReactPicker && (
        <div className="emoji-picker">
          {EMOJIS.map(e => (
            <button key={e} onClick={() => { onReact(e); setShowReactPicker(false); }}>
              {e}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
