import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Identity } from 'spacetimedb';
import { DbConnection, tables } from './module_bindings';
import { useTable, useSpacetimeDB } from 'spacetimedb/react';

const TYPING_TIMEOUT_MS = 3000;

function App() {
  const { isActive, identity: myIdentity, token } = useSpacetimeDB();
  const [conn, setConn] = useState<DbConnection | null>(null);
  const [subscribed, setSubscribed] = useState(false);
  const [registerName, setRegisterName] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageText, setMessageText] = useState('');
  const [error, setError] = useState('');
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const [users] = useTable(tables.user);
  const [rooms] = useTable(tables.room);
  const [roomMembers] = useTable(tables.roomMember);
  const [messages] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [readReceipts] = useTable(tables.readReceipt);

  // Save token and subscribe when connected
  useEffect(() => {
    if (!isActive || !token) return;
    localStorage.setItem('auth_token', token);
  }, [isActive, token]);

  // Get the connection object from the provider for calling reducers
  useEffect(() => {
    if (!isActive || !myIdentity) return;
    // The provider manages the connection; we get it via DbConnection
    const connection = DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withDatabaseName('')
      .build() as unknown;
    // Actually we need to get the existing connection from the context
    // For now, subscribe via the connection we find
  }, [isActive, myIdentity]);

  // Current user
  const currentUser = useMemo(() => {
    if (!myIdentity || !users) return null;
    return users.find(u => u.identity.toHexString() === myIdentity.toHexString()) ?? null;
  }, [users, myIdentity]);

  // My room memberships
  const myMemberships = useMemo(() => {
    if (!myIdentity || !roomMembers) return [];
    return roomMembers.filter(m => m.userIdentity.toHexString() === myIdentity.toHexString());
  }, [roomMembers, myIdentity]);

  const myRoomIds = useMemo(() => new Set(myMemberships.map(m => m.roomId)), [myMemberships]);

  // Rooms I've joined
  const myRooms = useMemo(() => {
    if (!rooms) return [];
    return rooms.filter(r => myRoomIds.has(r.id));
  }, [rooms, myRoomIds]);

  // Rooms I haven't joined
  const availableRooms = useMemo(() => {
    if (!rooms) return [];
    return rooms.filter(r => !myRoomIds.has(r.id));
  }, [rooms, myRoomIds]);

  // Messages for selected room, sorted by time
  const roomMessages = useMemo(() => {
    if (selectedRoomId === null || !messages) return [];
    return messages
      .filter(m => m.roomId === selectedRoomId)
      .sort((a, b) => Number(a.createdAt - b.createdAt));
  }, [messages, selectedRoomId]);

  // Members of selected room
  const selectedRoomMembers = useMemo(() => {
    if (selectedRoomId === null || !roomMembers || !users) return [];
    const memberIdentities = roomMembers
      .filter(m => m.roomId === selectedRoomId)
      .map(m => m.userIdentity.toHexString());
    return users.filter(u => memberIdentities.includes(u.identity.toHexString()));
  }, [roomMembers, users, selectedRoomId]);

  // Online users
  const onlineUsers = useMemo(() => {
    if (!users) return [];
    return users.filter(u => u.online);
  }, [users]);

  // Typing indicators for selected room (filter out self and expired)
  const roomTyping = useMemo(() => {
    if (selectedRoomId === null || !typingIndicators || !myIdentity || !users) return [];
    const now = BigInt(Date.now()) * 1000n; // microseconds
    return typingIndicators
      .filter(ti =>
        ti.roomId === selectedRoomId &&
        ti.userIdentity.toHexString() !== myIdentity.toHexString() &&
        (now - ti.startedAt) < BigInt(TYPING_TIMEOUT_MS) * 1000n
      )
      .map(ti => {
        const user = users.find(u => u.identity.toHexString() === ti.userIdentity.toHexString());
        return user?.name ?? 'Unknown';
      });
  }, [typingIndicators, selectedRoomId, myIdentity, users]);

  // Unread counts per room
  const unreadCounts = useMemo(() => {
    if (!myIdentity || !messages || !readReceipts) return new Map<bigint, number>();
    const counts = new Map<bigint, number>();
    for (const roomId of myRoomIds) {
      const myReceipt = readReceipts.find(
        rr => rr.roomId === roomId && rr.userIdentity.toHexString() === myIdentity.toHexString()
      );
      const lastReadId = myReceipt?.lastReadMessageId ?? 0n;
      const unread = messages.filter(m => m.roomId === roomId && m.id > lastReadId).length;
      if (unread > 0) counts.set(roomId, unread);
    }
    return counts;
  }, [messages, readReceipts, myIdentity, myRoomIds]);

  // Get "Seen by" names for a message
  const getSeenBy = useCallback((msg: { id: bigint; roomId: bigint; sender: Identity }): string[] => {
    if (!readReceipts || !users || !myIdentity) return [];
    return readReceipts
      .filter(rr =>
        rr.roomId === msg.roomId &&
        rr.lastReadMessageId >= msg.id &&
        rr.userIdentity.toHexString() !== msg.sender.toHexString()
      )
      .map(rr => {
        const user = users.find(u => u.identity.toHexString() === rr.userIdentity.toHexString());
        return user?.name ?? 'Unknown';
      });
  }, [readReceipts, users, myIdentity]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [roomMessages]);

  // Mark messages as read when viewing a room
  useEffect(() => {
    if (!conn || !subscribed || selectedRoomId === null || roomMessages.length === 0) return;
    const lastMsg = roomMessages[roomMessages.length - 1];
    conn.reducers.markRead({ roomId: selectedRoomId, messageId: lastMsg.id });
  }, [conn, subscribed, selectedRoomId, roomMessages]);

  // Periodically refresh typing indicators (force re-render to expire stale ones)
  const [, setTypingTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTypingTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // Handlers
  const handleRegister = useCallback(() => {
    if (!conn || !registerName.trim()) return;
    conn.reducers.register({ name: registerName.trim() });
    setRegisterName('');
    setError('');
  }, [conn, registerName]);

  const handleCreateRoom = useCallback(() => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({ name: newRoomName.trim() });
    setNewRoomName('');
  }, [conn, newRoomName]);

  const handleJoinRoom = useCallback((roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  }, [conn]);

  const handleLeaveRoom = useCallback((roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) setSelectedRoomId(null);
  }, [conn, selectedRoomId]);

  const handleSendMessage = useCallback(() => {
    if (!conn || !messageText.trim() || selectedRoomId === null) return;
    conn.reducers.sendMessage({ roomId: selectedRoomId, text: messageText.trim() });
    setMessageText('');
    // Clear typing
    if (typingTimerRef.current) {
      clearTimeout(typingTimerRef.current);
      typingTimerRef.current = null;
    }
  }, [conn, messageText, selectedRoomId]);

  const handleTyping = useCallback(() => {
    if (!conn || selectedRoomId === null) return;
    conn.reducers.setTyping({ roomId: selectedRoomId });
    if (typingTimerRef.current) clearTimeout(typingTimerRef.current);
    typingTimerRef.current = setTimeout(() => {
      conn.reducers.clearTyping({ roomId: selectedRoomId });
      typingTimerRef.current = null;
    }, TYPING_TIMEOUT_MS);
  }, [conn, selectedRoomId]);

  const handleSelectRoom = useCallback((roomId: bigint) => {
    setSelectedRoomId(roomId);
  }, []);

  const getUserName = useCallback((identity: Identity): string => {
    if (!users) return 'Unknown';
    const user = users.find(u => u.identity.toHexString() === identity.toHexString());
    return user?.name ?? 'Unknown';
  }, [users]);

  const formatTime = (microseconds: bigint): string => {
    const date = new Date(Number(microseconds / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Typing indicator text
  const typingText = useMemo(() => {
    if (roomTyping.length === 0) return '';
    if (roomTyping.length === 1) return `${roomTyping[0]} is typing...`;
    if (roomTyping.length === 2) return `${roomTyping[0]} and ${roomTyping[1]} are typing...`;
    return 'Multiple users are typing...';
  }, [roomTyping]);

  if (!connected || !subscribed) {
    return (
      <div className="app loading">
        <div className="loading-spinner">Connecting to SpacetimeDB...</div>
        {error && <div className="error">{error}</div>}
      </div>
    );
  }

  if (!currentUser) {
    return (
      <div className="app register">
        <div className="register-card">
          <h1>Chat App</h1>
          <p>Choose a display name to get started</p>
          <div className="register-form">
            <input
              type="text"
              placeholder="Display name"
              value={registerName}
              onChange={e => setRegisterName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleRegister()}
              maxLength={50}
              autoFocus
            />
            <button onClick={handleRegister} disabled={!registerName.trim()}>
              Join
            </button>
          </div>
          {error && <div className="error">{error}</div>}
        </div>
      </div>
    );
  }

  const selectedRoom = rooms?.find(r => r.id === selectedRoomId);

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>Chat App</h2>
          <div className="current-user">Logged in as <strong>{currentUser.name}</strong></div>
        </div>

        {/* Online Users */}
        <div className="sidebar-section">
          <h3>Online ({onlineUsers.length})</h3>
          <ul className="online-list">
            {onlineUsers.map(u => (
              <li key={u.identity.toHexString()}>
                <span className="online-dot" />
                {u.name}
                {u.identity.toHexString() === myIdentity?.toHexString() && ' (you)'}
              </li>
            ))}
          </ul>
        </div>

        {/* My Rooms */}
        <div className="sidebar-section">
          <h3>My Rooms</h3>
          <ul className="room-list">
            {myRooms.map(r => (
              <li
                key={String(r.id)}
                className={selectedRoomId === r.id ? 'active' : ''}
                onClick={() => handleSelectRoom(r.id)}
              >
                <span className="room-name">{r.name}</span>
                {unreadCounts.get(r.id) && (
                  <span className="unread-badge">{unreadCounts.get(r.id)}</span>
                )}
                <button
                  className="leave-btn"
                  onClick={e => { e.stopPropagation(); handleLeaveRoom(r.id); }}
                  title="Leave room"
                >
                  &times;
                </button>
              </li>
            ))}
          </ul>
        </div>

        {/* Available Rooms */}
        {availableRooms.length > 0 && (
          <div className="sidebar-section">
            <h3>Available Rooms</h3>
            <ul className="room-list">
              {availableRooms.map(r => (
                <li key={String(r.id)}>
                  <span className="room-name">{r.name}</span>
                  <button className="join-btn" onClick={() => handleJoinRoom(r.id)}>Join</button>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* Create Room */}
        <div className="sidebar-section">
          <h3>Create Room</h3>
          <div className="create-room-form">
            <input
              type="text"
              placeholder="Room name"
              value={newRoomName}
              onChange={e => setNewRoomName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
              maxLength={100}
            />
            <button onClick={handleCreateRoom} disabled={!newRoomName.trim()}>+</button>
          </div>
        </div>
      </div>

      {/* Main Chat Area */}
      <div className="main">
        {selectedRoom ? (
          <>
            <div className="chat-header">
              <h2>#{selectedRoom.name}</h2>
              <div className="room-members">
                {selectedRoomMembers.map(u => (
                  <span key={u.identity.toHexString()} className={`member ${u.online ? 'online' : 'offline'}`}>
                    {u.name}
                  </span>
                ))}
              </div>
            </div>

            <div className="messages">
              {roomMessages.length === 0 ? (
                <div className="no-messages">No messages yet. Say something!</div>
              ) : (
                roomMessages.map(msg => {
                  const isMe = msg.sender.toHexString() === myIdentity?.toHexString();
                  const seenBy = getSeenBy(msg);
                  return (
                    <div key={String(msg.id)} className={`message ${isMe ? 'mine' : ''}`}>
                      <div className="message-header">
                        <span className="message-author">{getUserName(msg.sender)}</span>
                        <span className="message-time">{formatTime(msg.createdAt)}</span>
                      </div>
                      <div className="message-text">{msg.text}</div>
                      {seenBy.length > 0 && (
                        <div className="seen-by">Seen by {seenBy.join(', ')}</div>
                      )}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {typingText && <div className="typing-indicator">{typingText}</div>}

            <div className="message-input">
              <input
                type="text"
                placeholder="Type a message..."
                value={messageText}
                onChange={e => {
                  setMessageText(e.target.value);
                  if (e.target.value.trim()) handleTyping();
                }}
                onKeyDown={e => e.key === 'Enter' && handleSendMessage()}
                autoFocus
              />
              <button onClick={handleSendMessage} disabled={!messageText.trim()}>Send</button>
            </div>
          </>
        ) : (
          <div className="no-room-selected">
            <h2>Welcome, {currentUser.name}!</h2>
            <p>Select a room from the sidebar or create a new one to start chatting.</p>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
