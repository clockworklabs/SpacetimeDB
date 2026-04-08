import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';

interface User {
  id: number;
  username: string;
}

interface Room {
  id: number;
  name: string;
}

interface Message {
  id: number;
  roomId: number;
  userId: number;
  username: string;
  content: string;
  createdAt: string;
}

interface ReadReceiptMap {
  [messageId: number]: { userId: number; username: string }[];
}

interface ScheduledMessage {
  id: number;
  roomId: number;
  content: string;
  scheduledAt: string;
  createdAt: string;
  roomName: string;
}

function App() {
  const [connected, setConnected] = useState(false);
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [loginName, setLoginName] = useState('');
  const [loginError, setLoginError] = useState('');

  const [rooms, setRooms] = useState<Room[]>([]);
  const [currentRoomId, setCurrentRoomId] = useState<number | null>(null);
  const [newRoomName, setNewRoomName] = useState('');

  const [messages, setMessages] = useState<Message[]>([]);
  const [messageInput, setMessageInput] = useState('');

  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [typingUsers, setTypingUsers] = useState<Map<number, string>>(new Map());
  const [readReceipts, setReadReceipts] = useState<ReadReceiptMap>({});
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [joinedRooms, setJoinedRooms] = useState<Set<number>>(new Set());
  const [scheduledMessages, setScheduledMessages] = useState<ScheduledMessage[]>([]);
  const [showSchedulePanel, setShowSchedulePanel] = useState(false);
  const [scheduleInput, setScheduleInput] = useState('');
  const [scheduleTime, setScheduleTime] = useState('');
  const [scheduleError, setScheduleError] = useState('');

  const socketRef = useRef<Socket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isTypingRef = useRef(false);
  const [showScrollBtn, setShowScrollBtn] = useState(false);

  // ── Socket setup ───────────────────────────────────────────────────────────
  useEffect(() => {
    const socket = io({ path: '/socket.io' });
    socketRef.current = socket;

    socket.on('connect', () => setConnected(true));
    socket.on('disconnect', () => setConnected(false));

    socket.on('online_users', (users: User[]) => {
      setOnlineUsers(users);
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

    return () => {
      socket.disconnect();
    };
  }, []);

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
    } catch {
      setLoginError('Connection error');
    }
  };

  // ── Rooms ──────────────────────────────────────────────────────────────────
  const loadRooms = async (userId: number) => {
    const [roomsRes, unreadRes] = await Promise.all([
      fetch('/api/rooms'),
      fetch(`/api/users/${userId}/unread`),
    ]);
    const roomsData: Room[] = await roomsRes.json();
    const unreadData: Record<number, number> = await unreadRes.json();
    setRooms(roomsData);
    setUnreadCounts(unreadData);

    // Track which rooms user is a member of
    const memberRes = await Promise.all(
      roomsData.map(r => fetch(`/api/rooms/${r.id}/members`).then(res => res.json() as Promise<number[]>).then(ids => ({ roomId: r.id, ids })))
    );
    const joined = new Set<number>();
    for (const { roomId, ids } of memberRes) {
      if (ids.includes(userId)) joined.add(roomId);
    }
    setJoinedRooms(joined);

    // Subscribe to all joined rooms via socket so new_message events arrive for unread tracking
    for (const roomId of joined) {
      socketRef.current?.emit('join_room', roomId);
    }
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
    if (!newRoomName.trim()) return;
    try {
      const res = await fetch('/api/rooms', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: newRoomName.trim() }),
      });
      if (res.ok) setNewRoomName('');
    } catch {}
  };

  const handleSelectRoom = async (roomId: number) => {
    if (!currentUser) return;
    if (currentRoomId === roomId) return;

    // Leave socket room
    if (currentRoomId !== null) {
      socketRef.current?.emit('leave_room', currentRoomId);
      // Clear typing for old room
      setTypingUsers(new Map());
    }

    setCurrentRoomId(roomId);
    setMessages([]);
    setReadReceipts({});

    // Join the room (DB + socket)
    if (!joinedRooms.has(roomId)) {
      await fetch(`/api/rooms/${roomId}/join`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id }),
      });
      setJoinedRooms(prev => new Set([...prev, roomId]));
    }
    socketRef.current?.emit('join_room', roomId);

    // Load messages
    const [msgsRes, receiptsRes] = await Promise.all([
      fetch(`/api/rooms/${roomId}/messages`),
      fetch(`/api/rooms/${roomId}/read-receipts?userId=${currentUser.id}`),
    ]);
    const msgs: Message[] = await msgsRes.json();
    const receipts: ReadReceiptMap = await receiptsRes.json();
    setMessages(msgs);
    setReadReceipts(receipts);
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
    setTypingUsers(new Map());
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
    try {
      await fetch(`/api/rooms/${currentRoomId}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userId: currentUser.id, content }),
      });
    } catch {}
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

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setMessageInput(e.target.value);
    if (e.target.value) startTyping();
    else stopTyping();
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

  return (
    <div className="app-layout">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <h2>PostgreSQL Chat</h2>
          <div className="user-info">
            <span className="status-dot" />
            <span>{currentUser.username}</span>
          </div>
        </div>

        <div className="sidebar-section">
          <div className="sidebar-section-title">Rooms</div>
        </div>

        <div className="room-list">
          {rooms.length === 0 && (
            <div style={{ padding: '8px', color: 'var(--text-muted)', fontSize: '0.82rem' }}>
              Create a room to get started
            </div>
          )}
          {rooms.map(room => (
            <div
              key={room.id}
              className={`room-item ${currentRoomId === room.id ? 'active' : ''}`}
              onClick={() => handleSelectRoom(room.id)}
            >
              <span className="room-name"># {room.name}</span>
              {unreadCounts[room.id] > 0 && currentRoomId !== room.id && (
                <span className="unread-badge">{unreadCounts[room.id]}</span>
              )}
            </div>
          ))}
        </div>

        <div className="create-room-form">
          <input
            type="text"
            placeholder="New room..."
            value={newRoomName}
            onChange={e => setNewRoomName(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
            maxLength={64}
          />
          <button onClick={handleCreateRoom}>+</button>
        </div>

        <div className="online-users">
          <div className="sidebar-section-title" style={{ marginBottom: '6px' }}>
            Online ({onlineUsers.length})
          </div>
          {onlineUsers.map(u => (
            <div key={u.id} className="online-user">
              <span className="status-dot" />
              <span>{u.username}</span>
            </div>
          ))}
        </div>
      </aside>

      {/* Main area */}
      <main className="main-area">
        {!currentRoom ? (
          <div className="no-room">
            <p>Select or create a room to start chatting</p>
          </div>
        ) : (
          <>
            <div className="room-header">
              <h3># {currentRoom.name}</h3>
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
                      return (
                        <div key={msg.id} className="message-item">
                          <div className="message-content">{msg.content}</div>
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

            <div className="input-bar">
              <input
                type="text"
                placeholder={`Message #${currentRoom.name}`}
                value={messageInput}
                onChange={handleInputChange}
                onKeyDown={handleKeyDown}
                maxLength={2000}
                autoFocus
              />
              <button onClick={handleSend} disabled={!messageInput.trim()}>Send</button>
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
