import { useState, useEffect, useRef, useCallback } from 'react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';
import type {
  User,
  Room,
  RoomMember,
  Message,
  MessageEditHistory,
  ReadReceipt,
  MessageReaction,
  ScheduledMessage,
} from './module_bindings/types';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';

// ── Helpers ──────────────────────────────────────────────────────────────────

function tsToMs(ts: { microsSinceUnixEpoch: bigint }): number {
  return Number(ts.microsSinceUnixEpoch / 1000n);
}

function idHex(id: Identity): string {
  return id.toHexString();
}

function timeAgo(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

function statusColor(status: string, online: boolean): string {
  if (!online || status === 'invisible') return '#6e7681';
  if (status === 'away') return '#d29922';
  if (status === 'dnd') return '#F85149';
  return '#3FB950';
}

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

// ── Main Component ────────────────────────────────────────────────────────────

export default function App() {
  const { isActive, identity: myIdentity, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;
  const [subscribed, setSubscribed] = useState(false);

  // Save auth token
  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  // Subscribe to all tables
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        'SELECT * FROM user',
        'SELECT * FROM room',
        'SELECT * FROM room_member',
        'SELECT * FROM message',
        'SELECT * FROM message_edit_history',
        'SELECT * FROM typing_indicator',
        'SELECT * FROM read_receipt',
        'SELECT * FROM message_reaction',
        'SELECT * FROM room_ban',
        'SELECT * FROM scheduled_message',
        'SELECT * FROM ephemeral_expiry',
      ]);
  }, [conn, isActive]);

  // Table data
  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [editHistories] = useTable(tables.messageEditHistory);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);
  const [reactions] = useTable(tables.messageReaction);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  // UI state
  const [nameInput, setNameInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [showHistoryFor, setShowHistoryFor] = useState<bigint | null>(null);
  const [showMembers, setShowMembers] = useState(false);
  const [tick, setTick] = useState(0); // force re-render for expiry/typing
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);

  // Heartbeat + periodic re-render for typing/ephemeral
  useEffect(() => {
    if (!conn || !isActive) return;
    conn.reducers.heartbeat({});
    const iv = setInterval(() => {
      conn.reducers.heartbeat({});
      setTick(t => t + 1);
    }, 5000);
    return () => clearInterval(iv);
  }, [conn, isActive]);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, selectedRoomId]);

  // Derived: current user
  const currentUser: User | undefined = myIdentity
    ? users.find(u => idHex(u.identity) === idHex(myIdentity))
    : undefined;

  // Mark room read when switching rooms
  const prevRoomRef = useRef<bigint | null>(null);
  useEffect(() => {
    if (!conn || !selectedRoomId || prevRoomRef.current === selectedRoomId) return;
    prevRoomRef.current = selectedRoomId;
    const roomMsgs = messages.filter(m => m.roomId === selectedRoomId);
    if (roomMsgs.length > 0) {
      const lastId = roomMsgs.reduce((max, m) => m.id > max ? m.id : max, 0n);
      conn.reducers.markRoomRead({ roomId: selectedRoomId, messageId: lastId });
    }
  }, [conn, selectedRoomId, messages]);

  // Mark room read when new messages arrive in the active room
  useEffect(() => {
    if (!conn || !selectedRoomId) return;
    const roomMsgs = messages.filter(m => m.roomId === selectedRoomId);
    if (roomMsgs.length > 0) {
      const lastId = roomMsgs.reduce((max, m) => m.id > max ? m.id : max, 0n);
      conn.reducers.markRoomRead({ roomId: selectedRoomId, messageId: lastId });
    }
  }, [conn, selectedRoomId, messages]);

  // Joined rooms
  const myMemberships: RoomMember[] = myIdentity
    ? roomMembers.filter(m => idHex(m.userId) === idHex(myIdentity))
    : [];
  const joinedRoomIds = new Set(myMemberships.map(m => m.roomId));

  // Unread counts per room
  function getUnreadCount(roomId: bigint): number {
    if (!myIdentity) return 0;
    const receipt = readReceipts.find(r => r.roomId === roomId && idHex(r.userId) === idHex(myIdentity));
    const lastRead = receipt?.messageId ?? 0n;
    return messages.filter(m => m.roomId === roomId && m.id > lastRead).length;
  }

  // Typing in current room (excluding self)
  function getTypingUsers(roomId: bigint): User[] {
    const now = Date.now();
    return typingIndicators
      .filter(i => i.roomId === roomId && (!myIdentity || idHex(i.userId) !== idHex(myIdentity)))
      .filter(i => now - tsToMs(i.lastTypingAt) < 6000)
      .map(i => users.find(u => idHex(u.identity) === idHex(i.userId)))
      .filter((u): u is User => u !== undefined);
  }

  // ── Handlers ──────────────────────────────────────────────────────────────

  function handleRegister() {
    if (!conn || !nameInput.trim()) return;
    conn.reducers.register({ name: nameInput.trim() });
    setNameInput('');
  }

  function handleCreateRoom() {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
  }

  function handleJoinRoom(roomId: bigint) {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
    setSelectedRoomId(roomId);
  }

  function handleLeaveRoom() {
    if (!conn || !selectedRoomId) return;
    conn.reducers.leaveRoom({ roomId: selectedRoomId });
    setSelectedRoomId(null);
  }

  function handleTyping() {
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      if (conn && selectedRoomId) conn.reducers.stopTyping({ roomId: selectedRoomId });
    }, 5000);
  }

  function handleSendMessage() {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;
    if (showSchedule && scheduleTime) {
      const sendAtMicros = BigInt(new Date(scheduleTime).getTime()) * 1000n;
      conn.reducers.scheduleMessage({ roomId: selectedRoomId, text: messageInput.trim(), sendAtMicros });
      setShowSchedule(false);
      setScheduleTime('');
    } else {
      conn.reducers.sendMessage({
        roomId: selectedRoomId,
        text: messageInput.trim(),
        isEphemeral,
        durationSecs: isEphemeral ? ephemeralDuration : 0,
      });
    }
    conn.reducers.stopTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    setMessageInput('');
  }

  function handleEditSave(messageId: bigint) {
    if (!conn || !editText.trim()) return;
    conn.reducers.editMessage({ messageId, newText: editText.trim() });
    setEditingMessageId(null);
    setEditText('');
  }

  function handleReact(messageId: bigint, emoji: string) {
    if (!conn) return;
    conn.reducers.reactToMessage({ messageId, emoji });
  }

  function handleKick(roomId: bigint, userId: Identity) {
    if (!conn) return;
    conn.reducers.kickUser({ roomId, userId });
  }

  function handlePromote(roomId: bigint, userId: Identity) {
    if (!conn) return;
    conn.reducers.promoteUser({ roomId, userId });
  }

  function handleCancelScheduled(scheduledId: bigint) {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  }

  function handleSetStatus(status: string) {
    if (!conn) return;
    conn.reducers.setStatus({ status });
  }

  const handleMessageKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  }, [messageInput, selectedRoomId, isEphemeral, ephemeralDuration, showSchedule, scheduleTime, conn]);

  // ── Render helpers ────────────────────────────────────────────────────────

  function renderEphemeralCountdown(msg: Message): string | null {
    if (!msg.isEphemeral || !msg.expiresAt) return null;
    const remaining = tsToMs(msg.expiresAt) - Date.now();
    if (remaining <= 0) return 'expires soon';
    if (remaining < 60_000) return `expires in ${Math.ceil(remaining / 1000)}s`;
    return `expires in ${Math.ceil(remaining / 60_000)}m`;
  }

  function renderReactions(messageId: bigint) {
    const msgReactions: MessageReaction[] = reactions.filter(r => r.messageId === messageId);
    const grouped: Record<string, { count: number; users: string[] }> = {};
    for (const r of msgReactions) {
      if (!grouped[r.emoji]) grouped[r.emoji] = { count: 0, users: [] };
      grouped[r.emoji].count++;
      const u = users.find(u => idHex(u.identity) === idHex(r.userId));
      if (u) grouped[r.emoji].users.push(u.name);
    }
    const myHex = myIdentity ? idHex(myIdentity) : '';
    return Object.entries(grouped).map(([emoji, data]) => {
      const iReacted = myIdentity
        ? msgReactions.some(r => r.emoji === emoji && idHex(r.userId) === myHex)
        : false;
      return (
        <button
          key={emoji}
          className={`reaction-btn${iReacted ? ' reacted' : ''}`}
          title={data.users.join(', ')}
          onClick={() => handleReact(messageId, emoji)}
        >
          {emoji} {data.count}
        </button>
      );
    });
  }

  function renderReadReceipts(msg: Message) {
    if (!myIdentity) return null;
    const receipts: ReadReceipt[] = readReceipts.filter(r =>
      r.roomId === msg.roomId &&
      r.messageId >= msg.id &&
      idHex(r.userId) !== idHex(myIdentity)
    );
    if (receipts.length === 0) return null;
    const names = receipts
      .map(r => users.find(u => idHex(u.identity) === idHex(r.userId))?.name)
      .filter(Boolean)
      .join(', ');
    return <div className="read-receipt">Seen by {names}</div>;
  }

  // ── Registration Screen ───────────────────────────────────────────────────

  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="logo">SpacetimeDB Chat</div>
        <div className="loading-text">Connecting...</div>
      </div>
    );
  }

  if (!currentUser) {
    return (
      <div className="register-screen">
        <div className="register-card">
          <h1 className="logo">SpacetimeDB Chat</h1>
          <p className="register-subtitle">Enter your display name to get started</p>
          <input
            className="input"
            type="text"
            placeholder="Your name"
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            autoFocus
          />
          <button className="btn btn-primary" onClick={handleRegister} type="submit">
            Join
          </button>
        </div>
      </div>
    );
  }

  // ── Main Chat UI ──────────────────────────────────────────────────────────

  const selectedRoom: Room | undefined = selectedRoomId !== null ? rooms.find(r => r.id === selectedRoomId) : undefined;
  const roomMessages: Message[] = selectedRoomId
    ? messages.filter(m => m.roomId === selectedRoomId).sort((a, b) => Number(a.id - b.id))
    : [];
  const currentRoomMembers: RoomMember[] = selectedRoomId
    ? roomMembers.filter(m => m.roomId === selectedRoomId)
    : [];
  const myMembershipInRoom: RoomMember | undefined = selectedRoomId && myIdentity
    ? currentRoomMembers.find(m => idHex(m.userId) === idHex(myIdentity))
    : undefined;
  const isMemberOfSelected = myMembershipInRoom !== undefined;
  const isAdminInSelected = myMembershipInRoom?.isAdmin ?? false;
  const typingUsers = selectedRoomId ? getTypingUsers(selectedRoomId) : [];
  const myScheduled: ScheduledMessage[] = myIdentity
    ? scheduledMessages.filter(s => idHex(s.sender) === idHex(myIdentity))
    : [];

  // Re-render each second for countdowns
  void tick;

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <span className="logo-small">SpacetimeDB Chat</span>
        </div>

        {/* Status selector */}
        <div className="status-bar">
          <span
            className="status-dot"
            style={{ background: statusColor(currentUser.status, currentUser.online) }}
          />
          <span className="username">{currentUser.name}</span>
          <select
            className="status-select"
            value={currentUser.status}
            onChange={e => handleSetStatus(e.target.value)}
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        {/* Create room */}
        <div className="create-room">
          <input
            className="input input-sm"
            type="text"
            placeholder="Room name"
            value={newRoomName}
            onChange={e => setNewRoomName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
          />
          <button className="btn btn-icon" onClick={handleCreateRoom} title="Create room">+</button>
        </div>

        {/* Room list */}
        <div className="room-list">
          {rooms.map(room => {
            const joined = joinedRoomIds.has(room.id);
            const unread = getUnreadCount(room.id);
            const isSelected = selectedRoomId === room.id;
            return (
              <div
                key={String(room.id)}
                className={`room-item${isSelected ? ' selected' : ''}`}
                onClick={() => {
                  if (joined) {
                    setSelectedRoomId(room.id);
                    setShowMembers(false);
                  } else {
                    handleJoinRoom(room.id);
                  }
                }}
              >
                <span className="room-name">{room.name}</span>
                {!joined && <span className="join-hint">Join</span>}
                {joined && unread > 0 && (
                  <span className="unread-badge">{unread}</span>
                )}
              </div>
            );
          })}
        </div>

        {/* Scheduled messages section */}
        {myScheduled.length > 0 && (
          <div className="scheduled-section">
            <div className="section-title">Scheduled ({myScheduled.length})</div>
            {myScheduled.map(s => (
              <div key={String(s.scheduledId)} className="scheduled-item">
                <span className="scheduled-text">{s.text.slice(0, 30)}</span>
                <button
                  className="btn btn-xs btn-danger"
                  onClick={() => handleCancelScheduled(s.scheduledId)}
                >
                  Cancel
                </button>
              </div>
            ))}
            <div className="pending-label">Pending</div>
          </div>
        )}

        {/* Online users */}
        <div className="online-section">
          <div className="section-title">Online</div>
          {users.filter(u => u.online && u.status !== 'invisible').map(u => (
            <div key={idHex(u.identity)} className="user-item">
              <span
                className="status-dot"
                style={{ background: statusColor(u.status, u.online) }}
              />
              <span>{u.name}</span>
            </div>
          ))}
          {users.filter(u => !u.online).map(u => (
            <div key={idHex(u.identity)} className="user-item offline">
              <span className="status-dot" style={{ background: '#6e7681' }} />
              <span>{u.name}</span>
              <span className="last-active">Last active {timeAgo(tsToMs(u.lastActive))}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Main content */}
      <div className="main">
        {selectedRoomId && selectedRoom ? (
          <>
            {/* Room header */}
            <div className="room-header">
              <span className="room-title">{selectedRoom.name}</span>
              <div className="room-header-actions">
                <button className="btn btn-sm" onClick={() => setShowMembers(!showMembers)}>
                  Members
                </button>
                {isMemberOfSelected && (
                  <button className="btn btn-sm btn-danger" onClick={handleLeaveRoom}>
                    Leave
                  </button>
                )}
              </div>
            </div>

            {/* Members panel */}
            {showMembers && (
              <div className="members-panel">
                <div className="members-title">Members</div>
                {currentRoomMembers.map(member => {
                  const memberUser = users.find(u => idHex(u.identity) === idHex(member.userId));
                  const isMe = myIdentity ? idHex(member.userId) === idHex(myIdentity) : false;
                  return (
                    <div key={String(member.id)} className="member-item">
                      <span
                        className="status-dot"
                        style={{ background: memberUser ? statusColor(memberUser.status, memberUser.online) : '#6e7681' }}
                      />
                      <span>{memberUser?.name ?? '???'}</span>
                      {member.isAdmin && <span className="admin-badge">Admin</span>}
                      {isAdminInSelected && !isMe && !member.isAdmin && (
                        <>
                          <button
                            className="btn btn-xs btn-danger"
                            onClick={() => handleKick(member.roomId, member.userId)}
                          >
                            Kick
                          </button>
                          <button
                            className="btn btn-xs btn-primary"
                            onClick={() => handlePromote(member.roomId, member.userId)}
                          >
                            Promote
                          </button>
                        </>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

            {/* Messages */}
            <div className="messages">
              {roomMessages.map(msg => {
                const sender = users.find(u => idHex(u.identity) === idHex(msg.sender));
                const isMe = myIdentity ? idHex(msg.sender) === idHex(myIdentity) : false;
                const countdown = renderEphemeralCountdown(msg);
                const isEditing = editingMessageId === msg.id;
                const history: MessageEditHistory[] = editHistories.filter(h => h.messageId === msg.id);

                return (
                  <div key={String(msg.id)} className={`message${isMe ? ' mine' : ''}`}>
                    <div className="message-header">
                      <span
                        className="status-dot"
                        style={{ background: sender ? statusColor(sender.status, sender.online) : '#6e7681' }}
                      />
                      <span className="sender-name">{sender?.name ?? '???'}</span>
                      <span className="timestamp">
                        {new Date(tsToMs(msg.sentAt)).toLocaleTimeString()}
                      </span>
                      {countdown && (
                        <span className="ephemeral-indicator disappearing">{countdown}</span>
                      )}
                    </div>

                    {isEditing ? (
                      <div className="edit-form">
                        <input
                          className="input"
                          value={editText}
                          onChange={e => setEditText(e.target.value)}
                          onKeyDown={e => e.key === 'Enter' && handleEditSave(msg.id)}
                          autoFocus
                        />
                        <button className="btn btn-primary btn-sm" onClick={() => handleEditSave(msg.id)}>Save</button>
                        <button className="btn btn-sm" onClick={() => setEditingMessageId(null)}>Cancel</button>
                      </div>
                    ) : (
                      <div className="message-body">
                        <span className="message-text">{msg.text}</span>
                        {msg.editedAt && (
                          <span
                            className="edited-indicator"
                            style={{ cursor: 'pointer' }}
                            onClick={() => setShowHistoryFor(showHistoryFor === msg.id ? null : msg.id)}
                          >
                            {' '}(edited)
                          </span>
                        )}
                        {isMe && !isEditing && (
                          <button
                            className="btn btn-xs edit-btn"
                            onClick={() => { setEditingMessageId(msg.id); setEditText(msg.text); }}
                          >
                            Edit
                          </button>
                        )}
                      </div>
                    )}

                    {/* Edit history */}
                    {showHistoryFor === msg.id && history.length > 0 && (
                      <div className="edit-history">
                        <div className="edit-history-title">Edit History</div>
                        {history.map(h => (
                          <div key={String(h.id)} className="history-item">
                            <span className="history-text">{h.text}</span>
                            <span className="history-time">
                              {new Date(tsToMs(h.editedAt)).toLocaleTimeString()}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Reactions */}
                    <div className="reactions">
                      {renderReactions(msg.id)}
                      <div className="reaction-picker">
                        {EMOJIS.map(emoji => (
                          <button
                            key={emoji}
                            className="reaction-btn small"
                            onClick={() => handleReact(msg.id, emoji)}
                            title={`React with ${emoji}`}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Read receipts */}
                    {isMe && renderReadReceipts(msg)}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {typingUsers.length > 0 && (
              <div className="typing-indicator">
                {typingUsers.length === 1
                  ? `${typingUsers[0].name} is typing...`
                  : `Multiple users are typing...`}
              </div>
            )}

            {/* Input area */}
            {isMemberOfSelected && (
              <div className="input-area">
                <div className="input-controls">
                  {/* Ephemeral toggle */}
                  <select
                    className="ephemeral-select"
                    value={isEphemeral ? String(ephemeralDuration) : 'normal'}
                    onChange={e => {
                      if (e.target.value === 'normal') {
                        setIsEphemeral(false);
                      } else {
                        setIsEphemeral(true);
                        setEphemeralDuration(Number(e.target.value));
                      }
                    }}
                    title="Message expiry"
                  >
                    <option value="normal">Normal</option>
                    <option value="30">Ephemeral 30s</option>
                    <option value="60">Disappear 1m</option>
                    <option value="300">Expire 5m</option>
                  </select>

                  {/* Schedule toggle */}
                  <button
                    className={`btn btn-sm${showSchedule ? ' active' : ''}`}
                    onClick={() => setShowSchedule(!showSchedule)}
                    title="Schedule message"
                  >
                    Schedule
                  </button>
                </div>

                {showSchedule && (
                  <div className="schedule-row">
                    <input
                      type="datetime-local"
                      className="input input-sm"
                      value={scheduleTime}
                      onChange={e => setScheduleTime(e.target.value)}
                    />
                  </div>
                )}

                <div className="message-row">
                  <input
                    className="input message-input"
                    type="text"
                    placeholder="Type a message..."
                    value={messageInput}
                    onChange={e => { setMessageInput(e.target.value); handleTyping(); }}
                    onKeyDown={handleMessageKeyDown}
                  />
                  <button className="btn btn-primary" onClick={handleSendMessage}>
                    {showSchedule ? 'Schedule' : 'Send'}
                  </button>
                </div>
              </div>
            )}
          </>
        ) : (
          <div className="empty-state">
            <h2>SpacetimeDB Chat</h2>
            <p>Select a room or create a new one to start chatting.</p>
          </div>
        )}
      </div>
    </div>
  );
}
