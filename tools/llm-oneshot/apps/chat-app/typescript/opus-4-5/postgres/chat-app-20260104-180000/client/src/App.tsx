import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { api } from './api';
import { connectSocket, disconnectSocket, getSocket } from './socket';
import type {
  User,
  Room,
  Message,
  Reaction,
  RoomInvite,
  MessageEdit,
} from './types';

const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

export function App() {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [displayNameInput, setDisplayNameInput] = useState('');

  // Check for existing session
  useEffect(() => {
    const token = api.getToken();
    if (token) {
      api
        .getMe()
        .then(setUser)
        .catch(() => api.clearToken())
        .finally(() => setIsLoading(false));
    } else {
      setIsLoading(false);
    }
  }, []);

  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!displayNameInput.trim()) return;

    try {
      const { user, token } = await api.register(displayNameInput.trim());
      api.setToken(token);
      setUser(user);
    } catch (error) {
      console.error('Registration failed:', error);
    }
  };

  if (isLoading) {
    return (
      <div className="auth-container">
        <div className="auth-card">
          <p>Loading...</p>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="auth-container">
        <div className="auth-card">
          <h1>Welcome to Chat</h1>
          <p>Enter a display name to get started</p>
          <form className="auth-form" onSubmit={handleRegister}>
            <input
              type="text"
              className="input"
              placeholder="Display name"
              value={displayNameInput}
              onChange={e => setDisplayNameInput(e.target.value)}
              maxLength={50}
              autoFocus
            />
            <button
              type="submit"
              className="btn"
              disabled={!displayNameInput.trim()}
            >
              Join Chat
            </button>
          </form>
        </div>
      </div>
    );
  }

  return <ChatApp user={user} setUser={setUser} />;
}

interface ChatAppProps {
  user: User;
  setUser: (user: User | null) => void;
}

function ChatApp({ user, setUser }: ChatAppProps) {
  const [publicRooms, setPublicRooms] = useState<Room[]>([]);
  const [myRooms, setMyRooms] = useState<Room[]>([]);
  const [users, setUsers] = useState<User[]>([]);
  const [activeRoom, setActiveRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [typingUsers, setTypingUsers] = useState<Record<number, Set<string>>>(
    {}
  );
  const [invites, setInvites] = useState<RoomInvite[]>([]);
  const [showInvites, setShowInvites] = useState(false);
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showMembers, setShowMembers] = useState(false);
  const [members, setMembers] = useState<User[]>([]);
  const [reactions, setReactions] = useState<Record<number, Reaction[]>>({});
  const [readReceipts, setReadReceipts] = useState<Record<number, string[]>>(
    {}
  );
  const [replyCounts, setReplyCounts] = useState<Record<number, number>>({});
  const [threadView, setThreadView] = useState<{
    parentId: number;
    parentMessage: Message;
  } | null>(null);
  const [threadMessages, setThreadMessages] = useState<Message[]>([]);
  const [scheduledMessages, setScheduledMessages] = useState<Message[]>([]);

  // Initialize socket and fetch data
  useEffect(() => {
    const socket = connectSocket();

    const loadData = async () => {
      const [publicR, myR, allUsers, unread, inv] = await Promise.all([
        api.getPublicRooms(),
        api.getMyRooms(),
        api.getUsers(),
        api.getUnreadCounts(),
        api.getInvites(),
      ]);
      setPublicRooms(publicR);
      setMyRooms(myR);
      setUsers(allUsers);
      setUnreadCounts(unread);
      setInvites(inv);
    };

    loadData();

    // Socket event handlers
    socket.on('user:updated', (updatedUser: User) => {
      setUsers(prev => {
        const exists = prev.find(u => u.id === updatedUser.id);
        if (exists) {
          return prev.map(u => (u.id === updatedUser.id ? updatedUser : u));
        }
        return [...prev, updatedUser];
      });
      if (updatedUser.id === user.id) {
        setUser(updatedUser);
      }
    });

    socket.on('room:created', (room: Room) => {
      setPublicRooms(prev => [...prev, room]);
    });

    socket.on(
      'room:member:joined',
      ({ roomId, user: joinedUser }: { roomId: number; user: User }) => {
        setMembers(prev => {
          if (prev.find(m => m.id === joinedUser.id)) return prev;
          return [...prev, joinedUser];
        });
      }
    );

    socket.on(
      'room:member:left',
      ({ roomId, user: leftUser }: { roomId: number; user: User }) => {
        setMembers(prev => prev.filter(m => m.id !== leftUser.id));
      }
    );

    socket.on(
      'room:member:kicked',
      ({ roomId, user: kickedUser }: { roomId: number; user: User }) => {
        setMembers(prev => prev.filter(m => m.id !== kickedUser.id));
      }
    );

    socket.on(
      'room:member:banned',
      ({ roomId, user: bannedUser }: { roomId: number; user: User }) => {
        setMembers(prev => prev.filter(m => m.id !== bannedUser.id));
      }
    );

    socket.on(
      'room:member:promoted',
      ({ roomId, user: promotedUser }: { roomId: number; user: User }) => {
        setMembers(prev =>
          prev.map(m =>
            m.id === promotedUser.id ? { ...m, role: 'admin' as const } : m
          )
        );
      }
    );

    socket.on('room:kicked', ({ roomId }: { roomId: number }) => {
      setMyRooms(prev => prev.filter(r => r.id !== roomId));
      if (activeRoom?.id === roomId) {
        setActiveRoom(null);
        setMessages([]);
      }
    });

    socket.on('room:banned', ({ roomId }: { roomId: number }) => {
      setMyRooms(prev => prev.filter(r => r.id !== roomId));
      if (activeRoom?.id === roomId) {
        setActiveRoom(null);
        setMessages([]);
      }
    });

    socket.on(
      'room:invite',
      ({ room, invitedBy }: { room: Room; invitedBy: User }) => {
        setInvites(prev => [
          ...prev,
          {
            id: Date.now(),
            room,
            invitedBy,
            createdAt: new Date().toISOString(),
          },
        ]);
      }
    );

    socket.on('dm:created', ({ room }: { room: Room }) => {
      setMyRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
    });

    socket.on('message:created', (message: Message) => {
      setMessages(prev => {
        if (prev.find(m => m.id === message.id)) return prev;
        return [...prev, message];
      });
      // Update unread if not active room
      if (message.roomId !== activeRoom?.id && message.userId !== user.id) {
        setUnreadCounts(prev => ({
          ...prev,
          [message.roomId]: (prev[message.roomId] || 0) + 1,
        }));
      }
    });

    socket.on('message:updated', (message: Message) => {
      setMessages(prev => prev.map(m => (m.id === message.id ? message : m)));
    });

    socket.on(
      'message:deleted',
      ({ messageId, roomId }: { messageId: number; roomId: number }) => {
        setMessages(prev => prev.filter(m => m.id !== messageId));
      }
    );

    socket.on(
      'message:reactions:updated',
      ({
        messageId,
        reactions: newReactions,
      }: {
        messageId: number;
        reactions: Reaction[];
      }) => {
        setReactions(prev => ({ ...prev, [messageId]: newReactions }));
      }
    );

    socket.on(
      'message:read',
      ({ messageId, readers }: { messageId: number; readers: string[] }) => {
        setReadReceipts(prev => ({ ...prev, [messageId]: readers }));
      }
    );

    socket.on(
      'message:thread:updated',
      ({ parentId, replyCount }: { parentId: number; replyCount: number }) => {
        setReplyCounts(prev => ({ ...prev, [parentId]: replyCount }));
      }
    );

    socket.on(
      'message:scheduled:cancelled',
      ({ messageId, roomId }: { messageId: number; roomId: number }) => {
        setScheduledMessages(prev => prev.filter(m => m.id !== messageId));
      }
    );

    socket.on(
      'typing:update',
      ({
        roomId,
        userId,
        isTyping,
      }: {
        roomId: number;
        userId: string;
        isTyping: boolean;
      }) => {
        setTypingUsers(prev => {
          const roomTyping = new Set(prev[roomId] || []);
          if (isTyping) {
            roomTyping.add(userId);
          } else {
            roomTyping.delete(userId);
          }
          return { ...prev, [roomId]: roomTyping };
        });
      }
    );

    return () => {
      disconnectSocket();
    };
  }, [user.id]);

  // Load messages when room changes
  useEffect(() => {
    if (!activeRoom) {
      setMessages([]);
      setMembers([]);
      setScheduledMessages([]);
      return;
    }

    const loadRoomData = async () => {
      const socket = getSocket();
      socket?.emit('room:join', activeRoom.id);

      const [msgs, mems, scheduled] = await Promise.all([
        api.getMessages(activeRoom.id),
        api.getRoomMembers(activeRoom.id),
        api.getScheduledMessages(activeRoom.id),
      ]);

      setMessages(msgs);
      setMembers(mems);
      setScheduledMessages(scheduled);

      // Load reactions for messages
      const reactionsMap: Record<number, Reaction[]> = {};
      for (const msg of msgs.slice(-50)) {
        const r = await api.getReactions(msg.id);
        reactionsMap[msg.id] = r;
      }
      setReactions(reactionsMap);

      // Mark unread as read
      setUnreadCounts(prev => ({ ...prev, [activeRoom.id]: 0 }));

      // Mark last message as read
      if (msgs.length > 0) {
        const lastMsg = msgs[msgs.length - 1];
        api.markAsRead(lastMsg.id);
      }
    };

    loadRoomData();

    return () => {
      const socket = getSocket();
      socket?.emit('room:leave', activeRoom.id);
    };
  }, [activeRoom?.id]);

  // Load thread messages
  useEffect(() => {
    if (!threadView) {
      setThreadMessages([]);
      return;
    }

    api.getThread(threadView.parentId).then(setThreadMessages);
  }, [threadView?.parentId]);

  const handleJoinRoom = async (roomId: number) => {
    await api.joinRoom(roomId);
    const room = publicRooms.find(r => r.id === roomId);
    if (room) {
      setMyRooms(prev => [...prev, room]);
      setActiveRoom(room);
    }
  };

  const handleLeaveRoom = async () => {
    if (!activeRoom) return;
    await api.leaveRoom(activeRoom.id);
    setMyRooms(prev => prev.filter(r => r.id !== activeRoom.id));
    setActiveRoom(null);
  };

  const handleUpdateStatus = async (status: string) => {
    await api.updateStatus(status);
  };

  const handleAcceptInvite = async (inviteId: number) => {
    const result = await api.acceptInvite(inviteId);
    setInvites(prev => prev.filter(i => i.id !== inviteId));
    setMyRooms(prev => [...prev, result.room]);
    setActiveRoom(result.room);
    setShowInvites(false);
  };

  const handleDeclineInvite = async (inviteId: number) => {
    await api.declineInvite(inviteId);
    setInvites(prev => prev.filter(i => i.id !== inviteId));
  };

  const handleStartDM = async (targetUser: User) => {
    const room = await api.createDM(targetUser.id);
    setMyRooms(prev => {
      if (prev.find(r => r.id === room.id)) return prev;
      return [...prev, room];
    });
    setActiveRoom(room);
  };

  const currentTypingUsers = useMemo(() => {
    if (!activeRoom) return [];
    const typing = typingUsers[activeRoom.id];
    if (!typing || typing.size === 0) return [];
    return Array.from(typing)
      .filter(id => id !== user.id)
      .map(id => users.find(u => u.id === id)?.displayName)
      .filter(Boolean) as string[];
  }, [activeRoom?.id, typingUsers, users, user.id]);

  const onlineUsers = useMemo(
    () =>
      users.filter(
        u => u.status === 'online' || u.status === 'away' || u.status === 'dnd'
      ),
    [users]
  );

  return (
    <div className="app">
      <div className="sidebar">
        <div className="sidebar-header">
          <h1>💬 Chat</h1>
          {invites.length > 0 && (
            <button
              className="btn btn-small"
              onClick={() => setShowInvites(true)}
            >
              {invites.length} Invite{invites.length > 1 ? 's' : ''}
            </button>
          )}
        </div>

        <div className="sidebar-section">
          <h3>
            Your Rooms
            <button
              className="btn-icon"
              onClick={() => setShowCreateRoom(true)}
            >
              +
            </button>
          </h3>
          <div className="room-list">
            {myRooms.map(room => (
              <button
                key={room.id}
                className={`room-item ${activeRoom?.id === room.id ? 'active' : ''}`}
                onClick={() => setActiveRoom(room)}
              >
                <span className="room-icon">
                  {room.roomType === 'dm'
                    ? '👤'
                    : room.roomType === 'private'
                      ? '🔒'
                      : '#'}
                </span>
                <span className="room-name">{room.name}</span>
                {unreadCounts[room.id] > 0 && (
                  <span className="unread-badge">{unreadCounts[room.id]}</span>
                )}
              </button>
            ))}
            {myRooms.length === 0 && (
              <p
                style={{
                  color: 'var(--text-muted)',
                  fontSize: '0.85rem',
                  padding: '8px',
                }}
              >
                No rooms yet. Create or join one!
              </p>
            )}
          </div>
        </div>

        <div className="sidebar-section">
          <h3>Public Rooms</h3>
          <div className="room-list">
            {publicRooms
              .filter(r => !myRooms.find(m => m.id === r.id))
              .map(room => (
                <button
                  key={room.id}
                  className="room-item"
                  onClick={() => handleJoinRoom(room.id)}
                >
                  <span className="room-icon">#</span>
                  <span className="room-name">{room.name}</span>
                  <span
                    style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}
                  >
                    Join
                  </span>
                </button>
              ))}
          </div>
        </div>

        <div
          className="sidebar-section"
          style={{ flex: 1, overflow: 'hidden' }}
        >
          <h3>Online — {onlineUsers.length}</h3>
          <div className="user-list">
            {onlineUsers
              .filter(u => u.id !== user.id)
              .map(u => (
                <button
                  key={u.id}
                  className="user-item"
                  onClick={() => handleStartDM(u)}
                >
                  <span className={`status-dot ${u.status}`} />
                  <span className="room-name">{u.displayName}</span>
                  {u.status !== 'online' && (
                    <span className="last-active">
                      {u.status === 'away'
                        ? 'Away'
                        : u.status === 'dnd'
                          ? 'DND'
                          : ''}
                    </span>
                  )}
                </button>
              ))}
          </div>
        </div>

        <div className="user-profile">
          <div className="user-profile-info">
            <div className="user-profile-avatar">
              {user.displayName[0].toUpperCase()}
            </div>
            <div className="user-profile-details">
              <div className="user-profile-name">{user.displayName}</div>
            </div>
          </div>
          <div className="status-selector">
            {(['online', 'away', 'dnd', 'invisible'] as const).map(status => (
              <button
                key={status}
                className={`status-option ${user.status === status ? 'active' : ''}`}
                onClick={() => handleUpdateStatus(status)}
              >
                {status === 'online'
                  ? '🟢'
                  : status === 'away'
                    ? '🟡'
                    : status === 'dnd'
                      ? '🔴'
                      : '⚫'}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="main-content">
        {activeRoom ? (
          <ChatRoom
            room={activeRoom}
            user={user}
            messages={messages}
            members={members}
            reactions={reactions}
            readReceipts={readReceipts}
            replyCounts={replyCounts}
            typingUsers={currentTypingUsers}
            scheduledMessages={scheduledMessages}
            showMembers={showMembers}
            onToggleMembers={() => setShowMembers(!showMembers)}
            onLeave={handleLeaveRoom}
            onReaction={(messageId, emoji) =>
              api.toggleReaction(messageId, emoji)
            }
            onMarkRead={messageId => api.markAsRead(messageId)}
            onOpenThread={msg =>
              setThreadView({ parentId: msg.id, parentMessage: msg })
            }
            setScheduledMessages={setScheduledMessages}
            setMessages={setMessages}
            setReactions={setReactions}
          />
        ) : (
          <div className="empty-state">
            <h3>Welcome to Chat!</h3>
            <p>
              Select a room from the sidebar or create a new one to start
              chatting.
            </p>
          </div>
        )}
      </div>

      {threadView && (
        <ThreadPanel
          parentMessage={threadView.parentMessage}
          messages={threadMessages}
          user={user}
          onClose={() => setThreadView(null)}
          onSend={async content => {
            const msg = await api.sendMessage(
              threadView.parentMessage.roomId,
              content,
              { parentId: threadView.parentId }
            );
            setThreadMessages(prev => [...prev, msg]);
          }}
        />
      )}

      {showMembers && activeRoom && (
        <MembersPanel
          members={members}
          currentUser={user}
          room={activeRoom}
          onKick={userId => api.kickUser(activeRoom.id, userId)}
          onBan={userId => api.banUser(activeRoom.id, userId)}
          onPromote={userId => api.promoteUser(activeRoom.id, userId)}
          onInvite={username => api.inviteUser(activeRoom.id, username)}
        />
      )}

      {showInvites && (
        <Modal onClose={() => setShowInvites(false)} title="Pending Invites">
          {invites.length === 0 ? (
            <p style={{ color: 'var(--text-muted)' }}>No pending invites</p>
          ) : (
            invites.map(invite => (
              <div key={invite.id} className="invite-item">
                <div className="invite-info">
                  <div className="invite-room">{invite.room.name}</div>
                  <div className="invite-from">
                    From: {invite.invitedBy.displayName}
                  </div>
                </div>
                <div className="invite-actions">
                  <button
                    className="btn btn-small"
                    onClick={() => handleAcceptInvite(invite.id)}
                  >
                    Accept
                  </button>
                  <button
                    className="btn btn-small btn-secondary"
                    onClick={() => handleDeclineInvite(invite.id)}
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))
          )}
        </Modal>
      )}

      {showCreateRoom && (
        <CreateRoomModal
          onClose={() => setShowCreateRoom(false)}
          onCreate={async (name, type) => {
            const room = await api.createRoom(name, type);
            if (type === 'public') {
              setPublicRooms(prev => [...prev, room]);
            }
            setMyRooms(prev => [...prev, { ...room, role: 'admin' }]);
            setActiveRoom(room);
            setShowCreateRoom(false);
          }}
        />
      )}
    </div>
  );
}

interface ChatRoomProps {
  room: Room;
  user: User;
  messages: Message[];
  members: User[];
  reactions: Record<number, Reaction[]>;
  readReceipts: Record<number, string[]>;
  replyCounts: Record<number, number>;
  typingUsers: string[];
  scheduledMessages: Message[];
  showMembers: boolean;
  onToggleMembers: () => void;
  onLeave: () => void;
  onReaction: (messageId: number, emoji: string) => void;
  onMarkRead: (messageId: number) => void;
  onOpenThread: (message: Message) => void;
  setScheduledMessages: React.Dispatch<React.SetStateAction<Message[]>>;
  setMessages: React.Dispatch<React.SetStateAction<Message[]>>;
  setReactions: React.Dispatch<
    React.SetStateAction<Record<number, Reaction[]>>
  >;
}

function ChatRoom({
  room,
  user,
  messages,
  members,
  reactions,
  readReceipts,
  replyCounts,
  typingUsers,
  scheduledMessages,
  showMembers,
  onToggleMembers,
  onLeave,
  onReaction,
  onMarkRead,
  onOpenThread,
  setScheduledMessages,
  setMessages,
  setReactions,
}: ChatRoomProps) {
  const [messageInput, setMessageInput] = useState('');
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editingMessage, setEditingMessage] = useState<Message | null>(null);
  const [showSchedule, setShowSchedule] = useState(false);
  const [scheduledTime, setScheduledTime] = useState('');
  const [ephemeralMinutes, setEphemeralMinutes] = useState<number | null>(null);
  const [showEmojiPicker, setShowEmojiPicker] = useState<number | null>(null);
  const [showHistory, setShowHistory] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout>>();
  const isNearBottomRef = useRef(true);
  const containerRef = useRef<HTMLDivElement>(null);

  // Auto-scroll when near bottom
  useEffect(() => {
    if (isNearBottomRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  const handleScroll = () => {
    const container = containerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    isNearBottomRef.current = scrollHeight - scrollTop - clientHeight < 100;
  };

  const handleTyping = () => {
    const socket = getSocket();
    socket?.emit('typing:start', room.id);

    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }
    typingTimeoutRef.current = setTimeout(() => {
      socket?.emit('typing:stop', room.id);
    }, 3000);
  };

  const handleSend = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!messageInput.trim()) return;

    const socket = getSocket();
    socket?.emit('typing:stop', room.id);

    try {
      if (editingMessage) {
        const updated = await api.editMessage(
          editingMessage.id,
          messageInput.trim()
        );
        setMessages(prev => prev.map(m => (m.id === updated.id ? updated : m)));
        setEditingMessage(null);
      } else {
        const options: {
          parentId?: number;
          scheduledFor?: string;
          expiresInMinutes?: number;
        } = {};

        if (replyTo) {
          options.parentId = replyTo.id;
        }
        if (showSchedule && scheduledTime) {
          options.scheduledFor = new Date(scheduledTime).toISOString();
        }
        if (ephemeralMinutes) {
          options.expiresInMinutes = ephemeralMinutes;
        }

        const msg = await api.sendMessage(
          room.id,
          messageInput.trim(),
          options
        );

        if (msg.isScheduled) {
          setScheduledMessages(prev => [...prev, msg]);
        }

        setReplyTo(null);
        setShowSchedule(false);
        setScheduledTime('');
        setEphemeralMinutes(null);
      }

      setMessageInput('');
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  };

  const handleCancelScheduled = async (messageId: number) => {
    await api.cancelScheduledMessage(messageId);
    setScheduledMessages(prev => prev.filter(m => m.id !== messageId));
  };

  const handleShowHistory = async (messageId: number) => {
    const history = await api.getEditHistory(messageId);
    setEditHistory(history);
    setShowHistory(messageId);
  };

  const getMinDateTime = () => {
    const now = new Date();
    now.setMinutes(now.getMinutes() - now.getTimezoneOffset());
    return now.toISOString().slice(0, 16);
  };

  const isAdmin =
    room.role === 'admin' ||
    members.find(m => m.id === user.id)?.role === 'admin';
  const displayMessages = messages.filter(m => !m.parentId);

  return (
    <>
      <div className="chat-header">
        <h2>
          {room.roomType === 'dm'
            ? '👤'
            : room.roomType === 'private'
              ? '🔒'
              : '#'}{' '}
          {room.name}
        </h2>
        <div className="chat-header-actions">
          <button
            className="btn btn-small btn-secondary"
            onClick={onToggleMembers}
          >
            👥 {members.length}
          </button>
          <button className="btn btn-small btn-secondary" onClick={onLeave}>
            Leave
          </button>
        </div>
      </div>

      {scheduledMessages.length > 0 && (
        <div className="scheduled-panel" style={{ margin: '12px 24px' }}>
          <h4
            style={{
              fontSize: '0.85rem',
              marginBottom: '8px',
              color: 'var(--text-secondary)',
            }}
          >
            📅 Scheduled Messages
          </h4>
          {scheduledMessages.map(msg => (
            <div key={msg.id} className="scheduled-item">
              <div className="scheduled-content">{msg.content}</div>
              <div className="scheduled-time">
                {new Date(msg.scheduledFor!).toLocaleString()}
              </div>
              <button
                className="btn btn-small btn-danger"
                onClick={() => handleCancelScheduled(msg.id)}
              >
                Cancel
              </button>
            </div>
          ))}
        </div>
      )}

      <div
        className="messages-container"
        ref={containerRef}
        onScroll={handleScroll}
      >
        {displayMessages.map(msg => (
          <MessageItem
            key={msg.id}
            message={msg}
            isOwn={msg.userId === user.id}
            reactions={reactions[msg.id] || []}
            readers={readReceipts[msg.id] || []}
            replyCount={replyCounts[msg.id] || 0}
            showEmojiPicker={showEmojiPicker === msg.id}
            onToggleEmojiPicker={() =>
              setShowEmojiPicker(showEmojiPicker === msg.id ? null : msg.id)
            }
            onReaction={emoji => {
              onReaction(msg.id, emoji);
              setShowEmojiPicker(null);
            }}
            onReply={() => setReplyTo(msg)}
            onEdit={() => {
              setEditingMessage(msg);
              setMessageInput(msg.content);
            }}
            onOpenThread={() => onOpenThread(msg)}
            onShowHistory={() => handleShowHistory(msg.id)}
            onVisible={() => {
              if (msg.userId !== user.id) {
                onMarkRead(msg.id);
              }
            }}
          />
        ))}
        <div ref={messagesEndRef} />
      </div>

      <div className="typing-indicator">
        {typingUsers.length > 0 &&
          (typingUsers.length === 1
            ? `${typingUsers[0]} is typing...`
            : `${typingUsers.length} users are typing...`)}
      </div>

      <div className="message-input-container">
        {replyTo && (
          <div className="reply-indicator">
            <span className="reply-indicator-text">
              Replying to {replyTo.user.displayName}:{' '}
              {replyTo.content.slice(0, 50)}...
            </span>
            <button
              className="reply-indicator-close"
              onClick={() => setReplyTo(null)}
            >
              ×
            </button>
          </div>
        )}

        {editingMessage && (
          <div className="reply-indicator">
            <span className="reply-indicator-text">Editing message</span>
            <button
              className="reply-indicator-close"
              onClick={() => {
                setEditingMessage(null);
                setMessageInput('');
              }}
            >
              ×
            </button>
          </div>
        )}

        <form className="message-input-wrapper" onSubmit={handleSend}>
          <div className="message-input-field">
            <textarea
              className="message-input"
              placeholder={
                editingMessage ? 'Edit message...' : 'Type a message...'
              }
              value={messageInput}
              onChange={e => {
                setMessageInput(e.target.value);
                handleTyping();
              }}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  handleSend(e);
                }
              }}
              rows={1}
            />

            {!editingMessage && (
              <div className="message-options">
                <button
                  type="button"
                  className={`option-btn ${showSchedule ? 'active' : ''}`}
                  onClick={() => setShowSchedule(!showSchedule)}
                >
                  📅 Schedule
                </button>
                {showSchedule && (
                  <input
                    type="datetime-local"
                    className="input"
                    style={{ width: 'auto' }}
                    value={scheduledTime}
                    onChange={e => setScheduledTime(e.target.value)}
                    min={getMinDateTime()}
                  />
                )}
                <button
                  type="button"
                  className={`option-btn ${ephemeralMinutes === 1 ? 'active' : ''}`}
                  onClick={() =>
                    setEphemeralMinutes(ephemeralMinutes === 1 ? null : 1)
                  }
                >
                  ⏱️ 1m
                </button>
                <button
                  type="button"
                  className={`option-btn ${ephemeralMinutes === 5 ? 'active' : ''}`}
                  onClick={() =>
                    setEphemeralMinutes(ephemeralMinutes === 5 ? null : 5)
                  }
                >
                  ⏱️ 5m
                </button>
              </div>
            )}
          </div>

          <button type="submit" className="btn" disabled={!messageInput.trim()}>
            {editingMessage ? 'Save' : 'Send'}
          </button>
        </form>
      </div>

      {showHistory !== null && (
        <Modal onClose={() => setShowHistory(null)} title="Edit History">
          {editHistory.length === 0 ? (
            <p style={{ color: 'var(--text-muted)' }}>No edit history</p>
          ) : (
            editHistory.map(edit => (
              <div key={edit.id} className="edit-history-item">
                <div className="edit-history-content">
                  {edit.previousContent}
                </div>
                <div className="edit-history-time">
                  {new Date(edit.editedAt).toLocaleString()}
                </div>
              </div>
            ))
          )}
        </Modal>
      )}
    </>
  );
}

interface MessageItemProps {
  message: Message;
  isOwn: boolean;
  reactions: Reaction[];
  readers: string[];
  replyCount: number;
  showEmojiPicker: boolean;
  onToggleEmojiPicker: () => void;
  onReaction: (emoji: string) => void;
  onReply: () => void;
  onEdit: () => void;
  onOpenThread: () => void;
  onShowHistory: () => void;
  onVisible: () => void;
}

function MessageItem({
  message,
  isOwn,
  reactions,
  readers,
  replyCount,
  showEmojiPicker,
  onToggleEmojiPicker,
  onReaction,
  onReply,
  onEdit,
  onOpenThread,
  onShowHistory,
  onVisible,
}: MessageItemProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          onVisible();
          observer.disconnect();
        }
      },
      { threshold: 0.5 }
    );

    if (ref.current) {
      observer.observe(ref.current);
    }

    return () => observer.disconnect();
  }, [onVisible]);

  const timeRemaining = message.expiresAt
    ? Math.max(
        0,
        Math.floor((new Date(message.expiresAt).getTime() - Date.now()) / 1000)
      )
    : null;

  const [countdown, setCountdown] = useState(timeRemaining);

  useEffect(() => {
    if (!message.expiresAt) return;

    const interval = setInterval(() => {
      const remaining = Math.max(
        0,
        Math.floor((new Date(message.expiresAt!).getTime() - Date.now()) / 1000)
      );
      setCountdown(remaining);
    }, 1000);

    return () => clearInterval(interval);
  }, [message.expiresAt]);

  return (
    <div className="message" ref={ref}>
      <div className="message-avatar">
        {message.user.displayName[0].toUpperCase()}
      </div>
      <div className="message-content">
        <div className="message-header">
          <span className="message-author">{message.user.displayName}</span>
          <span className="message-time">
            {new Date(message.createdAt).toLocaleTimeString()}
          </span>
          {message.isEdited && (
            <span
              className="message-edited"
              onClick={onShowHistory}
              style={{ cursor: 'pointer' }}
            >
              (edited)
            </span>
          )}
        </div>
        <div className="message-text">{message.content}</div>

        {countdown !== null && countdown > 0 && (
          <div className="message-ephemeral">⏱️ Disappears in {countdown}s</div>
        )}

        {reactions.length > 0 && (
          <div className="reactions">
            {reactions.map(r => (
              <div
                key={r.emoji}
                className="reaction tooltip-container"
                onClick={() => onReaction(r.emoji)}
              >
                <span>{r.emoji}</span>
                <span className="reaction-count">{r.count}</span>
                <div className="tooltip">{r.users.join(', ')}</div>
              </div>
            ))}
          </div>
        )}

        {readers.length > 0 && (
          <div className="read-receipts">
            Seen by {readers.slice(0, 3).join(', ')}
            {readers.length > 3 ? ` +${readers.length - 3}` : ''}
          </div>
        )}

        {replyCount > 0 && (
          <div className="thread-preview" onClick={onOpenThread}>
            💬 {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
          </div>
        )}

        <div className="message-actions">
          <button className="message-action-btn" onClick={onToggleEmojiPicker}>
            😀
          </button>
          <button className="message-action-btn" onClick={onReply}>
            Reply
          </button>
          {isOwn && (
            <button className="message-action-btn" onClick={onEdit}>
              Edit
            </button>
          )}
          <button className="message-action-btn" onClick={onOpenThread}>
            Thread
          </button>
        </div>

        {showEmojiPicker && (
          <div className="emoji-picker">
            {EMOJIS.map(emoji => (
              <button
                key={emoji}
                className="emoji-btn"
                onClick={() => onReaction(emoji)}
              >
                {emoji}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

interface ThreadPanelProps {
  parentMessage: Message;
  messages: Message[];
  user: User;
  onClose: () => void;
  onSend: (content: string) => void;
}

function ThreadPanel({
  parentMessage,
  messages,
  user,
  onClose,
  onSend,
}: ThreadPanelProps) {
  const [input, setInput] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;
    onSend(input.trim());
    setInput('');
  };

  return (
    <div className="thread-container">
      <div className="thread-header">
        <h3>Thread</h3>
        <button className="btn-icon" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="thread-messages">
        <div
          className="message"
          style={{
            marginBottom: '16px',
            paddingBottom: '16px',
            borderBottom: '1px solid var(--border)',
          }}
        >
          <div className="message-avatar">
            {parentMessage.user.displayName[0].toUpperCase()}
          </div>
          <div className="message-content">
            <div className="message-header">
              <span className="message-author">
                {parentMessage.user.displayName}
              </span>
              <span className="message-time">
                {new Date(parentMessage.createdAt).toLocaleTimeString()}
              </span>
            </div>
            <div className="message-text">{parentMessage.content}</div>
          </div>
        </div>

        {messages.map(msg => (
          <div key={msg.id} className="message">
            <div
              className="message-avatar"
              style={{ width: '32px', height: '32px', fontSize: '0.75rem' }}
            >
              {msg.user.displayName[0].toUpperCase()}
            </div>
            <div className="message-content">
              <div className="message-header">
                <span className="message-author">{msg.user.displayName}</span>
                <span className="message-time">
                  {new Date(msg.createdAt).toLocaleTimeString()}
                </span>
              </div>
              <div className="message-text">{msg.content}</div>
            </div>
          </div>
        ))}
      </div>

      <div className="message-input-container">
        <form className="message-input-wrapper" onSubmit={handleSubmit}>
          <textarea
            className="message-input"
            placeholder="Reply in thread..."
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                handleSubmit(e);
              }
            }}
            rows={1}
          />
          <button type="submit" className="btn" disabled={!input.trim()}>
            Send
          </button>
        </form>
      </div>
    </div>
  );
}

interface MembersPanelProps {
  members: User[];
  currentUser: User;
  room: Room;
  onKick: (userId: string) => void;
  onBan: (userId: string) => void;
  onPromote: (userId: string) => void;
  onInvite: (username: string) => void;
}

function MembersPanel({
  members,
  currentUser,
  room,
  onKick,
  onBan,
  onPromote,
  onInvite,
}: MembersPanelProps) {
  const [inviteUsername, setInviteUsername] = useState('');
  const isAdmin = members.find(m => m.id === currentUser.id)?.role === 'admin';

  const handleInvite = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inviteUsername.trim()) return;
    onInvite(inviteUsername.trim());
    setInviteUsername('');
  };

  return (
    <div className="members-panel">
      <h3>Members — {members.length}</h3>

      {(room.roomType === 'private' || isAdmin) && (
        <form onSubmit={handleInvite} style={{ marginBottom: '16px' }}>
          <input
            type="text"
            className="input"
            placeholder="Invite by username..."
            value={inviteUsername}
            onChange={e => setInviteUsername(e.target.value)}
            style={{ marginBottom: '8px' }}
          />
          <button
            type="submit"
            className="btn btn-small"
            style={{ width: '100%' }}
          >
            Invite
          </button>
        </form>
      )}

      {members.map(member => (
        <div key={member.id} className="member-item">
          <div className="member-avatar">
            {member.displayName[0].toUpperCase()}
            <div className={`member-status status-dot ${member.status}`} />
          </div>
          <div className="member-info">
            <div className="member-name">{member.displayName}</div>
            {member.role === 'admin' && (
              <div className="member-role">Admin</div>
            )}
          </div>
          {isAdmin && member.id !== currentUser.id && (
            <div style={{ display: 'flex', gap: '4px' }}>
              {member.role !== 'admin' && (
                <button
                  className="btn-icon"
                  onClick={() => onPromote(member.id)}
                  title="Promote to admin"
                >
                  ⬆️
                </button>
              )}
              <button
                className="btn-icon"
                onClick={() => onKick(member.id)}
                title="Kick"
              >
                👢
              </button>
              <button
                className="btn-icon"
                onClick={() => onBan(member.id)}
                title="Ban"
              >
                🚫
              </button>
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

interface ModalProps {
  onClose: () => void;
  title: string;
  children: React.ReactNode;
}

function Modal({ onClose, title, children }: ModalProps) {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>{title}</h3>
        {children}
        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

interface CreateRoomModalProps {
  onClose: () => void;
  onCreate: (name: string, type: 'public' | 'private') => void;
}

function CreateRoomModal({ onClose, onCreate }: CreateRoomModalProps) {
  const [name, setName] = useState('');
  const [type, setType] = useState<'public' | 'private'>('public');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    onCreate(name.trim(), type);
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Create Room</h3>
        <form onSubmit={handleSubmit}>
          <input
            type="text"
            className="input"
            placeholder="Room name"
            value={name}
            onChange={e => setName(e.target.value)}
            maxLength={100}
            autoFocus
            style={{ marginBottom: '16px' }}
          />
          <div style={{ display: 'flex', gap: '8px', marginBottom: '16px' }}>
            <button
              type="button"
              className={`option-btn ${type === 'public' ? 'active' : ''}`}
              onClick={() => setType('public')}
              style={{ flex: 1 }}
            >
              # Public
            </button>
            <button
              type="button"
              className={`option-btn ${type === 'private' ? 'active' : ''}`}
              onClick={() => setType('private')}
              style={{ flex: 1 }}
            >
              🔒 Private
            </button>
          </div>
          <div className="modal-actions">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onClose}
            >
              Cancel
            </button>
            <button type="submit" className="btn" disabled={!name.trim()}>
              Create
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
