import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import type { Message, User, Room, RoomMember, ScheduledMessage, TypingStatus, RoomLastRead, MessageReaction, MessageHistory } from './module_bindings/types';

const TYPING_EXPIRE_MS = 5000;
const AWAY_TIMEOUT_MS = 5 * 60 * 1000;

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
        'SELECT * FROM room_ban',
        'SELECT * FROM message',
        'SELECT * FROM message_history',
        'SELECT * FROM message_reaction',
        'SELECT * FROM typing_status',
        'SELECT * FROM read_receipt',
        'SELECT * FROM room_last_read',
        'SELECT * FROM scheduled_message',
      ]);
  }, [conn, isActive]);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [messageHistories] = useTable(tables.messageHistory);
  const [messageReactions] = useTable(tables.messageReaction);
  const [typingStatuses] = useTable(tables.typingStatus);
  const [roomLastReads] = useTable(tables.roomLastRead);
  const [scheduledMessages] = useTable(tables.scheduledMessage);

  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [showSchedulePanel, setShowSchedulePanel] = useState(false);
  const [scheduleTime, setScheduleTime] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState<bigint | null>(null);
  const [editText, setEditText] = useState('');
  const [historyMessageId, setHistoryMessageId] = useState<bigint | null>(null);
  const [showScheduledList, setShowScheduledList] = useState(false);
  const [now, setNow] = useState(Date.now());
  const [lastActivity, setLastActivity] = useState(Date.now());
  const [myStatus, setMyStatus] = useState('online');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [nameInput, setNameInput] = useState('');

  const myHex = myIdentity?.toHexString();
  const myUser = users.find(u => u.identity.toHexString() === myHex);
  const isRegistered = !!myUser;

  // Update clock every second for typing expiry and presence display
  useEffect(() => {
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, []);

  // Auto-away timer
  useEffect(() => {
    if (!conn || !myUser) return;
    const interval = setInterval(() => {
      if (Date.now() - lastActivity > AWAY_TIMEOUT_MS && myUser.status === 'online') {
        conn.reducers.updateStatus({ status: 'away' });
      }
    }, 30000);
    return () => clearInterval(interval);
  }, [conn, myUser, lastActivity]);

  // Track user activity
  const handleActivity = useCallback(() => {
    setLastActivity(Date.now());
    if (conn && myUser && myUser.status === 'away') {
      conn.reducers.updateStatus({ status: 'online' });
    }
  }, [conn, myUser]);

  // Mark room as read when selected
  useEffect(() => {
    if (conn && selectedRoomId !== null && subscribed) {
      conn.reducers.markRoomRead({ roomId: selectedRoomId });
    }
  }, [conn, selectedRoomId, messages.length, subscribed]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length, selectedRoomId]);

  // Registration
  const handleRegister = () => {
    if (!conn || !nameInput.trim()) return;
    conn.reducers.register({ name: nameInput.trim() });
  };

  // Send message
  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageText.trim()) return;
    handleActivity();
    if (isEphemeral) {
      conn.reducers.sendEphemeralMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        durationSecs: ephemeralDuration,
      });
    } else if (showSchedulePanel && scheduleTime) {
      const sendAtMs = new Date(scheduleTime).getTime();
      const sendAtMicros = BigInt(sendAtMs) * 1000n;
      conn.reducers.scheduleMessage({
        roomId: selectedRoomId,
        text: messageText.trim(),
        sendAtMicros,
      });
      setShowSchedulePanel(false);
    } else {
      conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    }
    setMessageText('');
  };

  // Handle typing indicator
  const handleMessageInput = (val: string) => {
    setMessageText(val);
    handleActivity();
    if (!conn || !selectedRoomId) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.stopTyping({ roomId: selectedRoomId });
    }, 3000);
  };

  // Status change
  const handleStatusChange = (status: string) => {
    if (!conn) return;
    setMyStatus(status);
    conn.reducers.updateStatus({ status });
  };

  // Room management
  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
    setShowCreateRoom(false);
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = () => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.leaveRoom({ roomId: selectedRoomId });
    setSelectedRoomId(null);
  };

  const handleSelectRoom = (roomId: bigint) => {
    setSelectedRoomId(roomId);
    handleActivity();
  };

  // Admin actions
  const handleKickUser = (userIdentity: string) => {
    if (!conn || !selectedRoomId) return;
    const target = users.find(u => u.identity.toHexString() === userIdentity);
    if (!target) return;
    conn.reducers.kickUser({ roomId: selectedRoomId, targetIdentity: target.identity });
  };

  const handlePromoteUser = (userIdentity: string) => {
    if (!conn || !selectedRoomId) return;
    const target = users.find(u => u.identity.toHexString() === userIdentity);
    if (!target) return;
    conn.reducers.promoteUser({ roomId: selectedRoomId, targetIdentity: target.identity });
  };

  // Message editing
  const handleEditMessage = (msg: Message) => {
    setEditingMessageId(msg.id);
    setEditText(msg.text);
  };

  const handleSaveEdit = () => {
    if (!conn || !editingMessageId || !editText.trim()) return;
    conn.reducers.editMessage({ messageId: editingMessageId, newText: editText.trim() });
    setEditingMessageId(null);
    setEditText('');
  };

  // Reactions
  const handleReact = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    handleActivity();
    conn.reducers.reactToMessage({ messageId, emoji });
  };

  // Cancel scheduled
  const handleCancelScheduled = (scheduledId: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ scheduledId });
  };

  // Computed data
  const myRooms = useMemo(() => {
    const memberRoomIds = new Set(
      roomMembers.filter(m => m.userIdentity.toHexString() === myHex).map(m => m.roomId.toString())
    );
    return rooms.filter(r => memberRoomIds.has(r.id.toString()));
  }, [rooms, roomMembers, myHex]);

  const unreadCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const room of myRooms) {
      const lastRead = roomLastReads.find(
        lr => lr.userIdentity.toHexString() === myHex && lr.roomId === room.id
      );
      const lastReadId = lastRead?.lastReadMessageId ?? 0n;
      counts[room.id.toString()] = messages.filter(
        m => m.roomId === room.id && m.id > lastReadId
      ).length;
    }
    return counts;
  }, [myRooms, messages, roomLastReads, myHex]);

  const currentRoomMessages = useMemo(() => {
    if (!selectedRoomId) return [];
    return [...messages.filter(m => m.roomId === selectedRoomId)].sort(
      (a, b) => Number(a.sentAt.microsSinceUnixEpoch - b.sentAt.microsSinceUnixEpoch)
    );
  }, [messages, selectedRoomId]);

  const currentRoomMembers = useMemo(() => {
    if (!selectedRoomId) return [];
    return roomMembers.filter(m => m.roomId === selectedRoomId);
  }, [roomMembers, selectedRoomId]);

  const amAdmin = useMemo(() => {
    if (!selectedRoomId || !myHex) return false;
    return currentRoomMembers.some(
      m => m.userIdentity.toHexString() === myHex && m.role === 'admin'
    );
  }, [currentRoomMembers, myHex, selectedRoomId]);

  const activeTypers = useMemo(() => {
    if (!selectedRoomId) return [];
    return typingStatuses.filter(ts => {
      if (ts.roomId !== selectedRoomId) return false;
      if (ts.userIdentity.toHexString() === myHex) return false;
      const lastTypedMs = Number(ts.lastTypedAt.microsSinceUnixEpoch / 1000n);
      return now - lastTypedMs < TYPING_EXPIRE_MS;
    });
  }, [typingStatuses, selectedRoomId, myHex, now]);

  const myScheduledMessages = useMemo(() => {
    return scheduledMessages.filter(sm => sm.sender.toHexString() === myHex);
  }, [scheduledMessages, myHex]);

  const getSeenBy = useCallback((msg: Message) => {
    if (!selectedRoomId) return [];
    return roomLastReads
      .filter(lr => lr.roomId === msg.roomId && lr.lastReadMessageId >= msg.id)
      .map(lr => {
        const u = users.find(u => u.identity.toHexString() === lr.userIdentity.toHexString());
        return u?.name;
      })
      .filter((name): name is string => !!name && name !== myUser?.name);
  }, [roomLastReads, users, myUser, selectedRoomId]);

  const getReactions = useCallback((msgId: bigint) => {
    const reacts = messageReactions.filter(r => r.messageId === msgId);
    const grouped: Record<string, { count: number; users: string[]; mine: boolean }> = {};
    for (const r of reacts) {
      if (!grouped[r.emoji]) grouped[r.emoji] = { count: 0, users: [], mine: false };
      grouped[r.emoji].count++;
      const uname = users.find(u => u.identity.toHexString() === r.userIdentity.toHexString())?.name || 'Unknown';
      grouped[r.emoji].users.push(uname);
      if (r.userIdentity.toHexString() === myHex) grouped[r.emoji].mine = true;
    }
    return grouped;
  }, [messageReactions, users, myHex]);

  const getHistory = useCallback((msgId: bigint) => {
    return [...messageHistories.filter(h => h.messageId === msgId)].sort(
      (a, b) => Number(a.editedAt.microsSinceUnixEpoch - b.editedAt.microsSinceUnixEpoch)
    );
  }, [messageHistories]);

  const formatTime = (ts: { microsSinceUnixEpoch: bigint }) => {
    const d = new Date(Number(ts.microsSinceUnixEpoch / 1000n));
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatLastActive = (user: User) => {
    if (user.online && user.status !== 'invisible') return null;
    const ms = Number(user.lastActive.microsSinceUnixEpoch / 1000n);
    const diff = now - ms;
    if (diff < 60000) return 'Last active just now';
    if (diff < 3600000) return `Last active ${Math.floor(diff / 60000)}m ago`;
    return `Last active ${Math.floor(diff / 3600000)}h ago`;
  };

  const statusDot = (status: string, online: boolean) => {
    if (!online || status === 'invisible') return '#6f7987';
    if (status === 'away') return '#fbdc8e';
    if (status === 'dnd') return '#ff4c4c';
    return '#4cf490';
  };

  const getStatusLabel = (status: string, online: boolean) => {
    if (!online || status === 'invisible') return 'Offline';
    if (status === 'away') return 'Away';
    if (status === 'dnd') return 'Do Not Disturb';
    return 'Online';
  };

  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="loading-logo">SpacetimeDB Chat</div>
        <div className="loading-text">Connecting...</div>
      </div>
    );
  }

  if (!isRegistered) {
    return (
      <div className="login-screen">
        <div className="login-card">
          <h1>SpacetimeDB Chat</h1>
          <p>Enter your display name to join</p>
          <input
            className="input"
            placeholder="Enter your name"
            value={nameInput}
            onChange={e => setNameInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleRegister()}
            autoFocus
          />
          <button className="btn btn-primary" onClick={handleRegister}>
            Join
          </button>
        </div>
      </div>
    );
  }

  const selectedRoom = rooms.find(r => r.id === selectedRoomId);

  return (
    <div className="app" onMouseMove={handleActivity} onKeyDown={handleActivity}>
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h2 className="app-title">SpacetimeDB Chat</h2>
        </div>

        {/* Current user + status */}
        <div className="user-profile">
          <div className="user-profile-name">
            <span
              className="status-dot"
              style={{ background: statusDot(myUser?.status || 'online', true) }}
            />
            <span>{myUser?.name}</span>
          </div>
          <select
            className="status-select"
            value={myUser?.status || 'online'}
            onChange={e => handleStatusChange(e.target.value)}
          >
            <option value="online">Online</option>
            <option value="away">Away</option>
            <option value="dnd">Do Not Disturb</option>
            <option value="invisible">Invisible</option>
          </select>
        </div>

        {/* Rooms */}
        <div className="sidebar-section">
          <div className="sidebar-section-header">
            <span>Rooms</span>
            <button className="btn-icon" onClick={() => setShowCreateRoom(!showCreateRoom)} title="Create room">
              +
            </button>
          </div>

          {showCreateRoom && (
            <div className="create-room-form">
              <input
                className="input"
                placeholder="Room name"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                autoFocus
              />
              <button className="btn btn-primary btn-sm" onClick={handleCreateRoom}>
                Create
              </button>
            </div>
          )}

          <div className="room-list">
            {rooms.map(room => {
              const isMember = myRooms.some(r => r.id === room.id);
              const unread = unreadCounts[room.id.toString()] || 0;
              const isSelected = selectedRoomId === room.id;
              return (
                <div
                  key={room.id.toString()}
                  className={`room-item ${isSelected ? 'active' : ''}`}
                >
                  <span
                    className="room-name"
                    onClick={() => isMember ? handleSelectRoom(room.id) : undefined}
                    style={{ cursor: isMember ? 'pointer' : 'default', flex: 1 }}
                  >
                    {room.name}
                  </span>
                  {unread > 0 && isMember && (
                    <span className="badge">{unread}</span>
                  )}
                  {!isMember && (
                    <button className="btn-sm btn-outline" onClick={() => handleJoinRoom(room.id)}>
                      Join
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        {/* Scheduled messages */}
        {myScheduledMessages.length > 0 && (
          <div className="sidebar-section">
            <div
              className="sidebar-section-header clickable"
              onClick={() => setShowScheduledList(!showScheduledList)}
            >
              <span>Scheduled ({myScheduledMessages.length})</span>
              <span>{showScheduledList ? '▾' : '▸'}</span>
            </div>
            {showScheduledList && (
              <div className="scheduled-list">
                {myScheduledMessages.map(sm => {
                  const room = rooms.find(r => r.id === sm.roomId);
                  const sendAt = sm.scheduledAt.tag === 'Time' ? sm.scheduledAt.value.toDate() : new Date(0);
                  return (
                    <div key={sm.scheduledId.toString()} className="scheduled-item">
                      <div className="scheduled-item-info">
                        <span className="scheduled-room">{room?.name || '?'}</span>
                        <span className="scheduled-text">{sm.text}</span>
                        <span className="scheduled-time">{sendAt.toLocaleString()}</span>
                      </div>
                      <button
                        className="btn-sm btn-danger"
                        onClick={() => handleCancelScheduled(sm.scheduledId)}
                      >
                        Cancel
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </aside>

      {/* Main content */}
      <main className="main">
        {!selectedRoom ? (
          <div className="welcome">
            <h2>Welcome, {myUser?.name}!</h2>
            <p>Select a room or create one to start chatting.</p>
          </div>
        ) : (
          <>
            {/* Room header */}
            <div className="room-header">
              <div className="room-header-info">
                <h3 className="room-title">{selectedRoom.name}</h3>
                {amAdmin && <span className="admin-badge">Admin</span>}
              </div>
              <div className="room-header-actions">
                <button className="btn btn-outline btn-sm" onClick={handleLeaveRoom}>
                  Leave
                </button>
              </div>
            </div>

            {/* Messages */}
            <div className="messages-container">
              {currentRoomMessages.map(msg => {
                const sender = users.find(u => u.identity.toHexString() === msg.sender.toHexString());
                const isMe = msg.sender.toHexString() === myHex;
                const reactions = getReactions(msg.id);
                const seenBy = getSeenBy(msg);
                const history = getHistory(msg.id);
                const msUntilExpiry = msg.expiresAt
                  ? Number(msg.expiresAt.microsSinceUnixEpoch / 1000n) - now
                  : null;

                return (
                  <div key={msg.id.toString()} className={`message ${isMe ? 'message-mine' : ''}`}>
                    <div className="message-header">
                      <span className="message-sender">{sender?.name || 'Unknown'}</span>
                      <span className="message-time">{formatTime(msg.sentAt)}</span>
                      {msg.isEphemeral && msUntilExpiry !== null && (
                        <span className="ephemeral-badge" title="Disappearing message">
                          {msUntilExpiry > 0
                            ? `expires ${Math.ceil(msUntilExpiry / 1000)}s`
                            : 'expiring...'}
                        </span>
                      )}
                    </div>

                    {editingMessageId === msg.id ? (
                      <div className="edit-form">
                        <input
                          className="input"
                          value={editText}
                          onChange={e => setEditText(e.target.value)}
                          onKeyDown={e => e.key === 'Enter' && handleSaveEdit()}
                          autoFocus
                        />
                        <button className="btn btn-primary btn-sm" onClick={handleSaveEdit}>Save</button>
                        <button className="btn btn-outline btn-sm" onClick={() => setEditingMessageId(null)}>Cancel</button>
                      </div>
                    ) : (
                      <div className="message-body">
                        <span className="message-text">{msg.text}</span>
                        {msg.isEdited && (
                          <span
                            className="edited-indicator"
                            onClick={() => setHistoryMessageId(historyMessageId === msg.id ? null : msg.id)}
                            title="View edit history"
                          >
                            (edited)
                          </span>
                        )}
                        {isMe && !msg.isEphemeral && (
                          <button
                            className="btn-icon msg-action"
                            onClick={() => handleEditMessage(msg)}
                          >
                            Edit
                          </button>
                        )}
                      </div>
                    )}

                    {/* Edit history */}
                    {historyMessageId === msg.id && history.length > 0 && (
                      <div className="history-panel">
                        <div className="history-header">Edit History</div>
                        {history.map(h => (
                          <div key={h.id.toString()} className="history-entry">
                            <span className="history-text">{h.oldText}</span>
                            <span className="history-time">{formatTime(h.editedAt)}</span>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Reactions */}
                    <div className="reactions">
                      {Object.entries(reactions).map(([emoji, data]) => (
                        <button
                          key={emoji}
                          className={`reaction-btn ${data.mine ? 'mine' : ''}`}
                          onClick={() => handleReact(msg.id, emoji)}
                          title={data.users.join(', ')}
                        >
                          {emoji} {data.count}
                        </button>
                      ))}
                      <div className="reaction-picker">
                        {['👍', '❤️', '😂', '😮', '😢'].map(emoji => (
                          <button
                            key={emoji}
                            className="reaction-add-btn"
                            onClick={() => handleReact(msg.id, emoji)}
                            title={`React with ${emoji}`}
                          >
                            {emoji}
                          </button>
                        ))}
                      </div>
                    </div>

                    {/* Seen by */}
                    {seenBy.length > 0 && (
                      <div className="seen-by">
                        Seen by {seenBy.join(', ')}
                      </div>
                    )}
                  </div>
                );
              })}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {activeTypers.length > 0 && (
              <div className="typing-indicator">
                {activeTypers.length === 1
                  ? `${users.find(u => u.identity.toHexString() === activeTypers[0].userIdentity.toHexString())?.name || 'Someone'} is typing...`
                  : `${activeTypers.length} people are typing...`}
              </div>
            )}

            {/* Message input area */}
            <div className="input-area">
              <div className="input-controls">
                <label className="ephemeral-label" title="Send disappearing message">
                  <input
                    type="checkbox"
                    checked={isEphemeral}
                    onChange={e => {
                      setIsEphemeral(e.target.checked);
                      if (e.target.checked) setShowSchedulePanel(false);
                    }}
                  />
                  <span>Ephemeral</span>
                </label>
                {isEphemeral && (
                  <select
                    className="duration-select"
                    value={ephemeralDuration}
                    onChange={e => setEphemeralDuration(Number(e.target.value))}
                    aria-label="Select expiry duration"
                  >
                    <option value={30}>30s</option>
                    <option value={60}>1m</option>
                    <option value={300}>5m</option>
                    <option value={3600}>1h</option>
                  </select>
                )}
                <button
                  className="btn btn-outline btn-sm schedule-btn"
                  onClick={() => {
                    setShowSchedulePanel(!showSchedulePanel);
                    if (!showSchedulePanel) setIsEphemeral(false);
                  }}
                  title="Schedule message"
                >
                  Schedule
                </button>
              </div>

              {showSchedulePanel && (
                <div className="schedule-panel">
                  <input
                    type="datetime-local"
                    className="input"
                    value={scheduleTime}
                    onChange={e => setScheduleTime(e.target.value)}
                  />
                </div>
              )}

              <div className="message-input-row">
                <input
                  className="input message-input"
                  placeholder="Type a message..."
                  value={messageText}
                  onChange={e => handleMessageInput(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSendMessage();
                    }
                  }}
                />
                <button
                  className="btn btn-primary"
                  onClick={handleSendMessage}
                  disabled={!messageText.trim()}
                >
                  {showSchedulePanel ? 'Schedule' : 'Send'}
                </button>
              </div>
            </div>
          </>
        )}
      </main>

      {/* Users Panel */}
      <aside className="users-panel">
        <div className="panel-header">
          {selectedRoom ? (
            <>Members — {currentRoomMembers.length}</>
          ) : (
            <>Online — {users.filter(u => u.online && u.status !== 'invisible').length}</>
          )}
        </div>

        <div className="user-list">
          {selectedRoom ? (
            // Show room members
            currentRoomMembers.map(member => {
              const u = users.find(u => u.identity.toHexString() === member.userIdentity.toHexString());
              if (!u) return null;
              const isInvisible = u.status === 'invisible' && u.identity.toHexString() !== myHex;
              if (isInvisible) return null;
              const lastActive = formatLastActive(u);
              return (
                <div key={member.id.toString()} className="user-item">
                  <div className="user-item-main">
                    <span
                      className="status-dot"
                      style={{ background: statusDot(u.status, u.online) }}
                      title={getStatusLabel(u.status, u.online)}
                    />
                    <div className="user-item-info">
                      <span className="user-name">{u.name}</span>
                      {member.role === 'admin' && <span className="role-badge">Admin</span>}
                      {lastActive && <span className="last-active">{lastActive}</span>}
                    </div>
                  </div>
                  {amAdmin && u.identity.toHexString() !== myHex && (
                    <div className="admin-actions">
                      {member.role !== 'admin' && (
                        <>
                          <button
                            className="btn-sm btn-danger"
                            onClick={() => handleKickUser(u.identity.toHexString())}
                          >
                            Kick
                          </button>
                          <button
                            className="btn-sm btn-outline"
                            onClick={() => handlePromoteUser(u.identity.toHexString())}
                          >
                            Promote
                          </button>
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })
          ) : (
            // Show all users
            users
              .filter(u => u.status !== 'invisible' || u.identity.toHexString() === myHex)
              .map(u => {
                const lastActive = formatLastActive(u);
                return (
                  <div key={u.identity.toHexString()} className="user-item">
                    <span
                      className="status-dot"
                      style={{ background: statusDot(u.status, u.online) }}
                      title={getStatusLabel(u.status, u.online)}
                    />
                    <div className="user-item-info">
                      <span className="user-name">{u.name}</span>
                      {lastActive && <span className="last-active">{lastActive}</span>}
                    </div>
                  </div>
                );
              })
          )}
        </div>

        {/* Pending scheduled indicator */}
        {myScheduledMessages.length > 0 && (
          <div className="pending-scheduled">
            <span>Pending: {myScheduledMessages.length} scheduled</span>
          </div>
        )}
      </aside>

      {/* History modal */}
      {historyMessageId !== null && getHistory(historyMessageId).length === 0 && (
        <div className="modal-overlay" onClick={() => setHistoryMessageId(null)}>
          <div className="modal" onClick={e => e.stopPropagation()}>
            <p>No edit history available.</p>
            <button className="btn btn-outline" onClick={() => setHistoryMessageId(null)}>Close</button>
          </div>
        </div>
      )}
    </div>
  );
}
