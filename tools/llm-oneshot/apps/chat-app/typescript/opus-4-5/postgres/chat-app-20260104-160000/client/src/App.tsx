import { useState, useEffect, useRef, useCallback } from 'react';
import { io, Socket } from 'socket.io-client';
import { api } from './api';
import type {
  User,
  Room,
  Message,
  RoomMember,
  RoomInvitation,
  TypingUser,
  MessageEdit,
  ReadReceipt,
  Reaction,
} from './types';

const SOCKET_URL = 'http://localhost:3001';
const EMOJIS = ['👍', '❤️', '😂', '😮', '😢'];

export default function App() {
  const [token, setToken] = useState<string | null>(
    localStorage.getItem('token')
  );
  const [user, setUser] = useState<User | null>(null);
  const [socket, setSocket] = useState<Socket | null>(null);

  const [rooms, setRooms] = useState<Room[]>([]);
  const [selectedRoom, setSelectedRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [roomMembers, setRoomMembers] = useState<RoomMember[]>([]);
  const [onlineUsers, setOnlineUsers] = useState<User[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [invitations, setInvitations] = useState<RoomInvitation[]>([]);
  const [scheduledMessages, setScheduledMessages] = useState<Message[]>([]);
  const [typingUsers, setTypingUsers] = useState<TypingUser[]>([]);

  const [messageInput, setMessageInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [displayName, setDisplayName] = useState('');

  // Modals
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showInviteUser, setShowInviteUser] = useState(false);
  const [showUserSearch, setShowUserSearch] = useState(false);
  const [showEditHistory, setShowEditHistory] = useState<number | null>(null);
  const [showThreadPanel, setShowThreadPanel] = useState<Message | null>(null);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [showEphemeralModal, setShowEphemeralModal] = useState(false);
  const [showReadReceipts, setShowReadReceipts] = useState<number | null>(null);

  const [newRoomName, setNewRoomName] = useState('');
  const [isPrivateRoom, setIsPrivateRoom] = useState(false);
  const [userSearchQuery, setUserSearchQuery] = useState('');
  const [userSearchResults, setUserSearchResults] = useState<User[]>([]);
  const [editingMessage, setEditingMessage] = useState<Message | null>(null);
  const [editContent, setEditContent] = useState('');
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);
  const [threadReplies, setThreadReplies] = useState<Message[]>([]);
  const [readReceipts, setReadReceipts] = useState<ReadReceipt[]>([]);
  const [scheduleDate, setScheduleDate] = useState('');
  const [ephemeralMinutes, setEphemeralMinutes] = useState(1);
  const [showEmojiPicker, setShowEmojiPicker] = useState<number | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isNearBottomRef = useRef(true);

  // Initialize
  useEffect(() => {
    if (token) {
      api
        .getMe()
        .then(setUser)
        .catch(() => {
          localStorage.removeItem('token');
          setToken(null);
        });
    }
  }, [token]);

  // Socket connection
  useEffect(() => {
    if (!token) return;

    const newSocket = io(SOCKET_URL, {
      auth: { token },
    });

    newSocket.on('connect', () => {
      console.log('Connected to socket');
    });

    newSocket.on('room:created', (room: Room) => {
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
    });

    newSocket.on('message:created', (message: Message) => {
      setMessages(prev => {
        if (prev.find(m => m.id === message.id)) return prev;
        return [...prev, message];
      });
      if (message.userId !== user?.id) {
        setUnreadCounts(prev => ({
          ...prev,
          [message.roomId]: (prev[message.roomId] || 0) + 1,
        }));
      }
    });

    newSocket.on('message:updated', (message: Message) => {
      setMessages(prev =>
        prev.map(m => (m.id === message.id ? { ...m, ...message } : m))
      );
    });

    newSocket.on('message:deleted', ({ id }: { id: number }) => {
      setMessages(prev => prev.filter(m => m.id !== id));
    });

    newSocket.on(
      'message:replyAdded',
      ({ messageId }: { messageId: number }) => {
        setMessages(prev =>
          prev.map(m =>
            m.id === messageId ? { ...m, replyCount: m.replyCount + 1 } : m
          )
        );
      }
    );

    newSocket.on('reaction:added', (reaction: Reaction) => {
      setMessages(prev =>
        prev.map(m =>
          m.id === reaction.messageId
            ? { ...m, reactions: [...m.reactions, reaction] }
            : m
        )
      );
    });

    newSocket.on(
      'reaction:removed',
      ({
        messageId,
        userId,
        emoji,
      }: {
        messageId: number;
        userId: string;
        emoji: string;
      }) => {
        setMessages(prev =>
          prev.map(m =>
            m.id === messageId
              ? {
                  ...m,
                  reactions: m.reactions.filter(
                    r => !(r.userId === userId && r.emoji === emoji)
                  ),
                }
              : m
          )
        );
      }
    );

    newSocket.on(
      'messages:read',
      ({
        userId: readUserId,
        messageIds,
      }: {
        userId: string;
        messageIds: number[];
      }) => {
        // Update UI if needed
        if (readUserId === user?.id) {
          setUnreadCounts(prev => {
            const updated = { ...prev };
            if (selectedRoom) updated[selectedRoom.id] = 0;
            return updated;
          });
        }
      }
    );

    newSocket.on(
      'typing:started',
      ({ roomId, userId: typingUserId }: TypingUser) => {
        if (typingUserId !== user?.id) {
          setTypingUsers(prev => {
            if (
              prev.find(t => t.roomId === roomId && t.userId === typingUserId)
            )
              return prev;
            return [...prev, { roomId, userId: typingUserId }];
          });
        }
      }
    );

    newSocket.on(
      'typing:stopped',
      ({ roomId, userId: typingUserId }: TypingUser) => {
        setTypingUsers(prev =>
          prev.filter(t => !(t.roomId === roomId && t.userId === typingUserId))
        );
      }
    );

    newSocket.on('user:online', (onlineUser: User) => {
      setOnlineUsers(prev => {
        const existing = prev.find(u => u.id === onlineUser.id);
        if (existing) {
          return prev.map(u => (u.id === onlineUser.id ? onlineUser : u));
        }
        return [...prev, onlineUser];
      });
    });

    newSocket.on('user:offline', ({ userId }: { userId: string }) => {
      setOnlineUsers(prev => prev.filter(u => u.id !== userId));
    });

    newSocket.on(
      'user:status',
      ({
        userId,
        status,
        lastActive,
      }: {
        userId: string;
        status: string;
        lastActive: string;
      }) => {
        setOnlineUsers(prev =>
          prev.map(u =>
            u.id === userId
              ? { ...u, status: status as User['status'], lastActive }
              : u
          )
        );
      }
    );

    newSocket.on(
      'member:joined',
      ({
        roomId,
        userId: joinedUserId,
      }: {
        roomId: number;
        userId: string;
      }) => {
        if (selectedRoom?.id === roomId) {
          loadRoomMembers(roomId);
        }
      }
    );

    newSocket.on(
      'member:left',
      ({ roomId, userId: leftUserId }: { roomId: number; userId: string }) => {
        setRoomMembers(prev =>
          prev.filter(
            m => !(m.member.roomId === roomId && m.member.userId === leftUserId)
          )
        );
      }
    );

    newSocket.on(
      'member:kicked',
      ({
        roomId,
        userId: kickedUserId,
      }: {
        roomId: number;
        userId: string;
      }) => {
        setRoomMembers(prev =>
          prev.filter(
            m =>
              !(m.member.roomId === roomId && m.member.userId === kickedUserId)
          )
        );
      }
    );

    newSocket.on(
      'member:banned',
      ({
        roomId,
        userId: bannedUserId,
      }: {
        roomId: number;
        userId: string;
      }) => {
        setRoomMembers(prev =>
          prev.filter(
            m =>
              !(m.member.roomId === roomId && m.member.userId === bannedUserId)
          )
        );
      }
    );

    newSocket.on(
      'member:promoted',
      ({
        roomId,
        userId: promotedUserId,
      }: {
        roomId: number;
        userId: string;
      }) => {
        setRoomMembers(prev =>
          prev.map(m =>
            m.member.roomId === roomId && m.member.userId === promotedUserId
              ? { ...m, member: { ...m.member, role: 'admin' as const } }
              : m
          )
        );
      }
    );

    newSocket.on('room:kicked', ({ roomId }: { roomId: number }) => {
      setRooms(prev => prev.filter(r => r.id !== roomId));
      if (selectedRoom?.id === roomId) {
        setSelectedRoom(null);
        setMessages([]);
      }
    });

    newSocket.on('room:banned', ({ roomId }: { roomId: number }) => {
      setRooms(prev => prev.filter(r => r.id !== roomId));
      if (selectedRoom?.id === roomId) {
        setSelectedRoom(null);
        setMessages([]);
      }
    });

    newSocket.on(
      'invitation:received',
      (data: { invitation: any; room: Room; inviter: User }) => {
        setInvitations(prev => [
          ...prev,
          { invitation: data.invitation, room: data.room },
        ]);
      }
    );

    setSocket(newSocket);

    // Heartbeat for activity
    const heartbeat = setInterval(() => {
      newSocket.emit('heartbeat');
    }, 60000);

    return () => {
      clearInterval(heartbeat);
      newSocket.disconnect();
    };
  }, [token, user?.id]);

  // Load initial data
  useEffect(() => {
    if (!token) return;
    loadRooms();
    loadOnlineUsers();
    loadInvitations();
    loadUnreadCounts();
  }, [token]);

  // Load room data when selected
  useEffect(() => {
    if (!selectedRoom || !socket) return;
    socket.emit('room:join', selectedRoom.id);
    loadMessages(selectedRoom.id);
    loadRoomMembers(selectedRoom.id);
    loadScheduledMessages(selectedRoom.id);

    return () => {
      socket.emit('room:leave', selectedRoom.id);
    };
  }, [selectedRoom?.id, socket]);

  // Mark messages as read when viewing room
  useEffect(() => {
    if (!selectedRoom || messages.length === 0 || !user) return;
    const unreadMessages = messages.filter(m => m.userId !== user.id);
    if (unreadMessages.length > 0) {
      api.markAsRead(
        selectedRoom.id,
        unreadMessages.map(m => m.id)
      );
      setUnreadCounts(prev => ({ ...prev, [selectedRoom.id]: 0 }));
    }
  }, [selectedRoom?.id, messages, user]);

  // Auto-scroll
  useEffect(() => {
    if (isNearBottomRef.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages]);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const { scrollTop, scrollHeight, clientHeight } = e.currentTarget;
    isNearBottomRef.current = scrollHeight - scrollTop - clientHeight < 100;
  };

  // Countdown timer for ephemeral messages
  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick(t => t + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // API calls
  const loadRooms = async () => {
    try {
      const data = await api.getRooms();
      setRooms(data);
    } catch (err) {
      console.error('Failed to load rooms:', err);
    }
  };

  const loadMessages = async (roomId: number) => {
    try {
      setIsLoading(true);
      const data = await api.getMessages(roomId);
      setMessages(data);
    } catch (err) {
      console.error('Failed to load messages:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const loadRoomMembers = async (roomId: number) => {
    try {
      const data = await api.getRoomMembers(roomId);
      setRoomMembers(data);
    } catch (err) {
      console.error('Failed to load members:', err);
    }
  };

  const loadOnlineUsers = async () => {
    try {
      const data = await api.getOnlineUsers();
      setOnlineUsers(data);
    } catch (err) {
      console.error('Failed to load online users:', err);
    }
  };

  const loadInvitations = async () => {
    try {
      const data = await api.getInvitations();
      setInvitations(data);
    } catch (err) {
      console.error('Failed to load invitations:', err);
    }
  };

  const loadUnreadCounts = async () => {
    try {
      const data = await api.getUnreadCounts();
      setUnreadCounts(data);
    } catch (err) {
      console.error('Failed to load unread counts:', err);
    }
  };

  const loadScheduledMessages = async (roomId: number) => {
    try {
      const data = await api.getScheduledMessages(roomId);
      setScheduledMessages(data);
    } catch (err) {
      console.error('Failed to load scheduled messages:', err);
    }
  };

  // Handlers
  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!displayName.trim()) return;
    try {
      const { token: newToken, user: newUser } = await api.register(
        displayName.trim()
      );
      localStorage.setItem('token', newToken);
      setToken(newToken);
      setUser(newUser);
    } catch (err) {
      console.error('Registration failed:', err);
    }
  };

  const handleSendMessage = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!messageInput.trim() || !selectedRoom) return;

    try {
      await api.sendMessage(selectedRoom.id, messageInput.trim(), {
        parentMessageId: showThreadPanel?.id,
      });
      setMessageInput('');
      if (typingTimeoutRef.current) {
        clearTimeout(typingTimeoutRef.current);
        socket?.emit('typing:stop', selectedRoom.id);
      }
    } catch (err) {
      console.error('Failed to send message:', err);
    }
  };

  const handleScheduleMessage = async () => {
    if (!messageInput.trim() || !selectedRoom || !scheduleDate) return;
    try {
      await api.sendMessage(selectedRoom.id, messageInput.trim(), {
        scheduledFor: new Date(scheduleDate).toISOString(),
      });
      setMessageInput('');
      setShowScheduleModal(false);
      setScheduleDate('');
      loadScheduledMessages(selectedRoom.id);
    } catch (err) {
      console.error('Failed to schedule message:', err);
    }
  };

  const handleSendEphemeral = async () => {
    if (!messageInput.trim() || !selectedRoom) return;
    try {
      await api.sendMessage(selectedRoom.id, messageInput.trim(), {
        ephemeralMinutes,
      });
      setMessageInput('');
      setShowEphemeralModal(false);
    } catch (err) {
      console.error('Failed to send ephemeral message:', err);
    }
  };

  const handleCancelScheduled = async (messageId: number) => {
    try {
      await api.cancelScheduledMessage(messageId);
      setScheduledMessages(prev => prev.filter(m => m.id !== messageId));
    } catch (err) {
      console.error('Failed to cancel scheduled message:', err);
    }
  };

  const handleTyping = () => {
    if (!selectedRoom || !socket) return;
    socket.emit('typing:start', selectedRoom.id);

    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }
    typingTimeoutRef.current = setTimeout(() => {
      socket.emit('typing:stop', selectedRoom.id);
    }, 3000);
  };

  const handleCreateRoom = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newRoomName.trim()) return;
    try {
      const room = await api.createRoom(newRoomName.trim(), isPrivateRoom);
      setRooms(prev => [...prev, room]);
      setShowCreateRoom(false);
      setNewRoomName('');
      setIsPrivateRoom(false);
      setSelectedRoom(room);
    } catch (err) {
      console.error('Failed to create room:', err);
    }
  };

  const handleJoinRoom = async (room: Room) => {
    try {
      await api.joinRoom(room.id);
      setSelectedRoom(room);
    } catch (err) {
      console.error('Failed to join room:', err);
    }
  };

  const handleLeaveRoom = async () => {
    if (!selectedRoom) return;
    try {
      await api.leaveRoom(selectedRoom.id);
      setRooms(prev =>
        prev.filter(r => r.id !== selectedRoom.id || !r.isPrivate)
      );
      setSelectedRoom(null);
      setMessages([]);
    } catch (err) {
      console.error('Failed to leave room:', err);
    }
  };

  const handleSearchUsers = async () => {
    if (!userSearchQuery.trim()) return;
    try {
      const results = await api.searchUsers(userSearchQuery);
      setUserSearchResults(results);
    } catch (err) {
      console.error('Failed to search users:', err);
    }
  };

  const handleInviteUser = async (targetUserId: string) => {
    if (!selectedRoom) return;
    try {
      await api.inviteToRoom(selectedRoom.id, targetUserId);
      setShowInviteUser(false);
      setUserSearchQuery('');
      setUserSearchResults([]);
    } catch (err) {
      console.error('Failed to invite user:', err);
    }
  };

  const handleRespondToInvitation = async (
    invitation: RoomInvitation,
    accept: boolean
  ) => {
    try {
      await api.respondToInvitation(invitation.invitation.id, accept);
      setInvitations(prev =>
        prev.filter(i => i.invitation.id !== invitation.invitation.id)
      );
      if (accept) {
        loadRooms();
      }
    } catch (err) {
      console.error('Failed to respond to invitation:', err);
    }
  };

  const handleStartDM = async (targetUserId: string) => {
    try {
      const room = await api.createDM(targetUserId);
      setRooms(prev => {
        if (prev.find(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
      setSelectedRoom(room);
      setShowUserSearch(false);
    } catch (err) {
      console.error('Failed to start DM:', err);
    }
  };

  const handleKickUser = async (targetUserId: string) => {
    if (!selectedRoom) return;
    try {
      await api.kickFromRoom(selectedRoom.id, targetUserId);
    } catch (err) {
      console.error('Failed to kick user:', err);
    }
  };

  const handleBanUser = async (targetUserId: string) => {
    if (!selectedRoom) return;
    try {
      await api.banFromRoom(selectedRoom.id, targetUserId);
    } catch (err) {
      console.error('Failed to ban user:', err);
    }
  };

  const handlePromoteUser = async (targetUserId: string) => {
    if (!selectedRoom) return;
    try {
      await api.promoteInRoom(selectedRoom.id, targetUserId);
    } catch (err) {
      console.error('Failed to promote user:', err);
    }
  };

  const handleEditMessage = async () => {
    if (!editingMessage || !editContent.trim()) return;
    try {
      await api.editMessage(editingMessage.id, editContent.trim());
      setEditingMessage(null);
      setEditContent('');
    } catch (err) {
      console.error('Failed to edit message:', err);
    }
  };

  const handleShowEditHistory = async (messageId: number) => {
    try {
      const history = await api.getEditHistory(messageId);
      setEditHistory(history);
      setShowEditHistory(messageId);
    } catch (err) {
      console.error('Failed to load edit history:', err);
    }
  };

  const handleShowThread = async (message: Message) => {
    try {
      const replies = await api.getThreadReplies(message.id);
      setThreadReplies(replies);
      setShowThreadPanel(message);
    } catch (err) {
      console.error('Failed to load thread:', err);
    }
  };

  const handleShowReadReceipts = async (messageId: number) => {
    try {
      const receipts = await api.getReadReceipts(messageId);
      setReadReceipts(receipts);
      setShowReadReceipts(messageId);
    } catch (err) {
      console.error('Failed to load receipts:', err);
    }
  };

  const handleToggleReaction = async (messageId: number, emoji: string) => {
    try {
      await api.toggleReaction(messageId, emoji);
      setShowEmojiPicker(null);
    } catch (err) {
      console.error('Failed to toggle reaction:', err);
    }
  };

  const handleUpdateStatus = async (status: User['status']) => {
    try {
      await api.updateStatus(status);
      setUser(prev => (prev ? { ...prev, status } : null));
    } catch (err) {
      console.error('Failed to update status:', err);
    }
  };

  const formatTime = (date: string) => {
    return new Date(date).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const formatLastActive = (date: string) => {
    const diff = Date.now() - new Date(date).getTime();
    const minutes = Math.floor(diff / 60000);
    if (minutes < 1) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  };

  const getTimeRemaining = (expiresAt: string) => {
    const remaining = new Date(expiresAt).getTime() - Date.now();
    if (remaining <= 0) return 'Expiring...';
    const seconds = Math.floor(remaining / 1000);
    if (seconds < 60) return `${seconds}s`;
    return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  };

  const getTypingText = () => {
    if (!selectedRoom) return null;
    const typing = typingUsers.filter(t => t.roomId === selectedRoom.id);
    if (typing.length === 0) return null;
    if (typing.length === 1) {
      const typingUser = onlineUsers.find(u => u.id === typing[0].userId);
      return `${typingUser?.displayName || 'Someone'} is typing...`;
    }
    return 'Multiple users are typing...';
  };

  const getLocalDateTimeString = () => {
    const now = new Date();
    now.setMinutes(now.getMinutes() + 1);
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    return `${year}-${month}-${day}T${hours}:${minutes}`;
  };

  const currentMember = roomMembers.find(m => m.user.id === user?.id);
  const isAdmin = currentMember?.member.role === 'admin';

  // Login screen
  if (!token) {
    return (
      <div className="login-container">
        <div className="login-card animate-fade-in">
          <h1>💬 Chat App</h1>
          <p>Enter your display name to get started</p>
          <form className="login-form" onSubmit={handleRegister}>
            <input
              type="text"
              className="input"
              placeholder="Display name"
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              maxLength={50}
              autoFocus
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!displayName.trim()}
            >
              Join Chat
            </button>
          </form>
        </div>
      </div>
    );
  }

  // Group reactions by emoji
  const groupReactions = (reactions: Reaction[]) => {
    const grouped: Record<string, Reaction[]> = {};
    reactions.forEach(r => {
      if (!grouped[r.emoji]) grouped[r.emoji] = [];
      grouped[r.emoji].push(r);
    });
    return grouped;
  };

  return (
    <div className="app-container">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <h1>💬 Chat</h1>
          {user && (
            <div style={{ marginTop: 12 }}>
              <div style={{ fontSize: '0.9rem', fontWeight: 500 }}>
                {user.displayName}
              </div>
              <div className="status-selector">
                {(['online', 'away', 'dnd', 'invisible'] as const).map(
                  status => (
                    <button
                      key={status}
                      className={`status-option ${user.status === status ? 'active' : ''}`}
                      onClick={() => handleUpdateStatus(status)}
                    >
                      <span className={`user-status ${status}`} />
                      {status === 'dnd'
                        ? 'DND'
                        : status.charAt(0).toUpperCase() + status.slice(1)}
                    </button>
                  )
                )}
              </div>
            </div>
          )}
        </div>

        {/* Invitations */}
        {invitations.length > 0 && (
          <div className="sidebar-section">
            <div className="sidebar-section-title">
              Invitations{' '}
              <span className="invitation-badge">{invitations.length}</span>
            </div>
            {invitations.map(inv => (
              <div
                key={inv.invitation.id}
                className="room-item"
                style={{
                  flexDirection: 'column',
                  alignItems: 'stretch',
                  gap: 8,
                }}
              >
                <span>📩 {inv.room.name}</span>
                <div style={{ display: 'flex', gap: 8 }}>
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => handleRespondToInvitation(inv, true)}
                  >
                    Accept
                  </button>
                  <button
                    className="btn btn-sm btn-secondary"
                    onClick={() => handleRespondToInvitation(inv, false)}
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Rooms */}
        <div className="sidebar-section" style={{ flex: 1 }}>
          <div className="sidebar-section-title">
            Rooms
            <button
              className="btn-icon"
              onClick={() => setShowCreateRoom(true)}
              title="Create Room"
            >
              ➕
            </button>
          </div>
          <div className="room-list">
            {rooms
              .filter(r => !r.isDm)
              .map(room => (
                <button
                  key={room.id}
                  className={`room-item ${selectedRoom?.id === room.id ? 'active' : ''}`}
                  onClick={() => handleJoinRoom(room)}
                >
                  <span className="room-icon">
                    {room.isPrivate ? '🔒' : '#'}
                  </span>
                  <span
                    style={{
                      flex: 1,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {room.name}
                  </span>
                  {unreadCounts[room.id] > 0 && (
                    <span className="unread-badge">
                      {unreadCounts[room.id]}
                    </span>
                  )}
                </button>
              ))}
            {rooms.filter(r => !r.isDm).length === 0 && (
              <div className="empty-state" style={{ padding: 20 }}>
                <p>No rooms yet</p>
              </div>
            )}
          </div>
        </div>

        {/* DMs */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">
            Direct Messages
            <button
              className="btn-icon"
              onClick={() => setShowUserSearch(true)}
              title="New DM"
            >
              ➕
            </button>
          </div>
          <div className="room-list">
            {rooms
              .filter(r => r.isDm)
              .map(room => (
                <button
                  key={room.id}
                  className={`room-item ${selectedRoom?.id === room.id ? 'active' : ''}`}
                  onClick={() => setSelectedRoom(room)}
                >
                  <span className="room-icon">💬</span>
                  <span
                    style={{
                      flex: 1,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {room.name}
                  </span>
                  {unreadCounts[room.id] > 0 && (
                    <span className="unread-badge">
                      {unreadCounts[room.id]}
                    </span>
                  )}
                </button>
              ))}
          </div>
        </div>

        {/* Online Users */}
        <div className="sidebar-section">
          <div className="sidebar-section-title">
            Online — {onlineUsers.filter(u => u.status !== 'invisible').length}
          </div>
          <div className="user-list">
            {onlineUsers
              .filter(u => u.status !== 'invisible')
              .map(u => (
                <div
                  key={u.id}
                  className="user-item"
                  onClick={() => u.id !== user?.id && handleStartDM(u.id)}
                >
                  <span className={`user-status ${u.status}`} />
                  <div className="user-info">
                    <div className="user-name">{u.displayName}</div>
                    {u.status !== 'online' && (
                      <div className="user-last-active">
                        {formatLastActive(u.lastActive)}
                      </div>
                    )}
                  </div>
                </div>
              ))}
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="main-content">
        {selectedRoom ? (
          <>
            <div className="chat-header">
              <h2>
                {selectedRoom.isPrivate ? '🔒' : '#'} {selectedRoom.name}
              </h2>
              <div className="chat-header-actions">
                {selectedRoom.isPrivate && isAdmin && (
                  <button
                    className="btn btn-sm btn-secondary"
                    onClick={() => setShowInviteUser(true)}
                  >
                    Invite
                  </button>
                )}
                {!selectedRoom.isDm && (
                  <button
                    className="btn btn-sm btn-secondary"
                    onClick={handleLeaveRoom}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            {/* Scheduled Messages */}
            {scheduledMessages.length > 0 && (
              <div className="scheduled-messages">
                <div
                  style={{
                    fontSize: '0.8rem',
                    color: 'var(--text-muted)',
                    marginBottom: 8,
                  }}
                >
                  📅 Scheduled Messages
                </div>
                {scheduledMessages.map(msg => (
                  <div key={msg.id} className="scheduled-item">
                    <div className="scheduled-info">
                      <div>
                        {msg.content.slice(0, 50)}
                        {msg.content.length > 50 ? '...' : ''}
                      </div>
                      <div className="scheduled-time">
                        Sending at{' '}
                        {new Date(msg.scheduledFor!).toLocaleString()}
                      </div>
                    </div>
                    <button
                      className="btn btn-sm btn-danger"
                      onClick={() => handleCancelScheduled(msg.id)}
                    >
                      Cancel
                    </button>
                  </div>
                ))}
              </div>
            )}

            {/* Messages */}
            <div className="messages-container" onScroll={handleScroll}>
              {isLoading ? (
                <div className="loading">
                  <div className="spinner" />
                </div>
              ) : messages.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-state-icon">💬</div>
                  <h3>No messages yet</h3>
                  <p>Be the first to send a message!</p>
                </div>
              ) : (
                messages.map(message => {
                  const isOwn = message.userId === user?.id;
                  const grouped = groupReactions(message.reactions);

                  return (
                    <div
                      key={message.id}
                      className={`message ${isOwn ? 'own' : 'other'}`}
                    >
                      <div className="message-header">
                        <span className="message-author">
                          {message.user.displayName}
                        </span>
                        <span className="message-time">
                          {formatTime(message.createdAt)}
                        </span>
                        {message.isEdited && (
                          <span
                            className="message-edited"
                            style={{ cursor: 'pointer' }}
                            onClick={() => handleShowEditHistory(message.id)}
                          >
                            (edited)
                          </span>
                        )}
                      </div>
                      <div className="message-content">{message.content}</div>

                      <div className="message-footer">
                        {message.isEphemeral && message.expiresAt && (
                          <span className="ephemeral-indicator">
                            ⏱️ {getTimeRemaining(message.expiresAt)}
                          </span>
                        )}

                        {Object.entries(grouped).length > 0 && (
                          <div className="message-reactions">
                            {Object.entries(grouped).map(([emoji, reacts]) => (
                              <button
                                key={emoji}
                                className={`reaction ${reacts.some(r => r.userId === user?.id) ? 'own' : ''}`}
                                onClick={() =>
                                  handleToggleReaction(message.id, emoji)
                                }
                                title={reacts
                                  .map(r => r.user?.displayName || 'Unknown')
                                  .join(', ')}
                              >
                                {emoji}{' '}
                                <span className="reaction-count">
                                  {reacts.length}
                                </span>
                              </button>
                            ))}
                          </div>
                        )}

                        {message.replyCount > 0 && (
                          <span
                            className="thread-indicator"
                            onClick={() => handleShowThread(message)}
                          >
                            💬 {message.replyCount}{' '}
                            {message.replyCount === 1 ? 'reply' : 'replies'}
                          </span>
                        )}

                        <span
                          className="read-receipts"
                          style={{ cursor: 'pointer' }}
                          onClick={() => handleShowReadReceipts(message.id)}
                        >
                          ✓ Seen
                        </span>
                      </div>

                      <div className="message-actions">
                        <button
                          className="message-action-btn"
                          onClick={() =>
                            setShowEmojiPicker(
                              showEmojiPicker === message.id ? null : message.id
                            )
                          }
                        >
                          😀
                        </button>
                        <button
                          className="message-action-btn"
                          onClick={() => handleShowThread(message)}
                        >
                          Reply
                        </button>
                        {isOwn && (
                          <button
                            className="message-action-btn"
                            onClick={() => {
                              setEditingMessage(message);
                              setEditContent(message.content);
                            }}
                          >
                            Edit
                          </button>
                        )}
                      </div>

                      {showEmojiPicker === message.id && (
                        <div
                          className="emoji-picker"
                          style={{
                            position: 'absolute',
                            bottom: '100%',
                            left: 0,
                          }}
                        >
                          {EMOJIS.map(emoji => (
                            <button
                              key={emoji}
                              className="emoji-btn"
                              onClick={() =>
                                handleToggleReaction(message.id, emoji)
                              }
                            >
                              {emoji}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing Indicator */}
            {getTypingText() && (
              <div className="typing-indicator">{getTypingText()}</div>
            )}

            {/* Message Input */}
            <div className="message-input-area">
              <div className="input-actions">
                <button
                  className="btn btn-sm btn-secondary"
                  onClick={() => setShowScheduleModal(true)}
                >
                  📅 Schedule
                </button>
                <button
                  className="btn btn-sm btn-secondary"
                  onClick={() => setShowEphemeralModal(true)}
                >
                  ⏱️ Ephemeral
                </button>
              </div>
              <form className="input-row" onSubmit={handleSendMessage}>
                <textarea
                  className="message-input"
                  placeholder={
                    showThreadPanel ? 'Reply to thread...' : 'Type a message...'
                  }
                  value={messageInput}
                  onChange={e => {
                    setMessageInput(e.target.value);
                    handleTyping();
                  }}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSendMessage();
                    }
                  }}
                  rows={1}
                />
                <button
                  type="submit"
                  className="btn btn-primary"
                  disabled={!messageInput.trim()}
                >
                  Send
                </button>
              </form>
            </div>
          </>
        ) : (
          <div className="empty-state">
            <div className="empty-state-icon">💬</div>
            <h3>Select a room to start chatting</h3>
            <p>Or create a new room to get started</p>
          </div>
        )}
      </div>

      {/* Thread Panel */}
      {showThreadPanel && (
        <div className="thread-panel">
          <div className="thread-header">
            <h3>Thread</h3>
            <button
              className="modal-close"
              onClick={() => setShowThreadPanel(null)}
            >
              ×
            </button>
          </div>
          <div className="thread-parent">
            <div className="message-author">
              {showThreadPanel.user.displayName}
            </div>
            <div className="message-content">{showThreadPanel.content}</div>
          </div>
          <div className="messages-container" style={{ flex: 1 }}>
            {threadReplies.map(reply => (
              <div
                key={reply.id}
                className={`message ${reply.userId === user?.id ? 'own' : 'other'}`}
              >
                <div className="message-header">
                  <span className="message-author">
                    {reply.user.displayName}
                  </span>
                  <span className="message-time">
                    {formatTime(reply.createdAt)}
                  </span>
                </div>
                <div className="message-content">{reply.content}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Members Panel */}
      {selectedRoom && (
        <div className="members-panel">
          <h3>Members — {roomMembers.length}</h3>
          {roomMembers.map(member => (
            <div key={member.member.id} className="member-item">
              <span className={`user-status ${member.user.status}`} />
              <span className="user-name">{member.user.displayName}</span>
              {member.member.role === 'admin' && (
                <span className="member-role">Admin</span>
              )}
              {isAdmin && member.user.id !== user?.id && (
                <div style={{ marginLeft: 'auto', display: 'flex', gap: 4 }}>
                  {member.member.role !== 'admin' && (
                    <button
                      className="btn-icon"
                      onClick={() => handlePromoteUser(member.user.id)}
                      title="Promote"
                    >
                      ⬆️
                    </button>
                  )}
                  <button
                    className="btn-icon"
                    onClick={() => handleKickUser(member.user.id)}
                    title="Kick"
                  >
                    🚪
                  </button>
                  <button
                    className="btn-icon"
                    onClick={() => handleBanUser(member.user.id)}
                    title="Ban"
                  >
                    🚫
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Create Room Modal */}
      {showCreateRoom && (
        <div className="modal-overlay" onClick={() => setShowCreateRoom(false)}>
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Create Room</h3>
              <button
                className="modal-close"
                onClick={() => setShowCreateRoom(false)}
              >
                ×
              </button>
            </div>
            <form onSubmit={handleCreateRoom}>
              <div className="modal-body">
                <div className="input-group">
                  <label>Room Name</label>
                  <input
                    type="text"
                    className="input"
                    placeholder="e.g. General"
                    value={newRoomName}
                    onChange={e => setNewRoomName(e.target.value)}
                    maxLength={100}
                    autoFocus
                  />
                </div>
                <label
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 8,
                    cursor: 'pointer',
                  }}
                >
                  <input
                    type="checkbox"
                    checked={isPrivateRoom}
                    onChange={e => setIsPrivateRoom(e.target.checked)}
                  />
                  Private room (invite only)
                </label>
              </div>
              <div className="modal-footer">
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => setShowCreateRoom(false)}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="btn btn-primary"
                  disabled={!newRoomName.trim()}
                >
                  Create
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Invite User Modal */}
      {showInviteUser && (
        <div className="modal-overlay" onClick={() => setShowInviteUser(false)}>
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Invite User</h3>
              <button
                className="modal-close"
                onClick={() => setShowInviteUser(false)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <div className="input-group">
                <label>Search by username</label>
                <div style={{ display: 'flex', gap: 8 }}>
                  <input
                    type="text"
                    className="input"
                    placeholder="Enter username..."
                    value={userSearchQuery}
                    onChange={e => setUserSearchQuery(e.target.value)}
                    autoFocus
                  />
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={handleSearchUsers}
                  >
                    Search
                  </button>
                </div>
              </div>
              {userSearchResults.length > 0 && (
                <div className="user-list" style={{ marginTop: 12 }}>
                  {userSearchResults.map(u => (
                    <div
                      key={u.id}
                      className="user-item"
                      onClick={() => handleInviteUser(u.id)}
                    >
                      <span className={`user-status ${u.status}`} />
                      <span className="user-name">{u.displayName}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* User Search (DM) Modal */}
      {showUserSearch && (
        <div className="modal-overlay" onClick={() => setShowUserSearch(false)}>
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Start Direct Message</h3>
              <button
                className="modal-close"
                onClick={() => setShowUserSearch(false)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <div className="input-group">
                <label>Search for a user</label>
                <div style={{ display: 'flex', gap: 8 }}>
                  <input
                    type="text"
                    className="input"
                    placeholder="Enter username..."
                    value={userSearchQuery}
                    onChange={e => setUserSearchQuery(e.target.value)}
                    autoFocus
                  />
                  <button
                    type="button"
                    className="btn btn-primary"
                    onClick={handleSearchUsers}
                  >
                    Search
                  </button>
                </div>
              </div>
              {userSearchResults.length > 0 && (
                <div className="user-list" style={{ marginTop: 12 }}>
                  {userSearchResults
                    .filter(u => u.id !== user?.id)
                    .map(u => (
                      <div
                        key={u.id}
                        className="user-item"
                        onClick={() => handleStartDM(u.id)}
                      >
                        <span className={`user-status ${u.status}`} />
                        <span className="user-name">{u.displayName}</span>
                      </div>
                    ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Edit Message Modal */}
      {editingMessage && (
        <div className="modal-overlay" onClick={() => setEditingMessage(null)}>
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Edit Message</h3>
              <button
                className="modal-close"
                onClick={() => setEditingMessage(null)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <textarea
                className="message-input"
                value={editContent}
                onChange={e => setEditContent(e.target.value)}
                style={{ width: '100%', minHeight: 100 }}
                autoFocus
              />
            </div>
            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setEditingMessage(null)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleEditMessage}
                disabled={!editContent.trim()}
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Edit History Modal */}
      {showEditHistory !== null && (
        <div className="modal-overlay" onClick={() => setShowEditHistory(null)}>
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Edit History</h3>
              <button
                className="modal-close"
                onClick={() => setShowEditHistory(null)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              {editHistory.length === 0 ? (
                <p style={{ color: 'var(--text-muted)' }}>
                  No edit history available
                </p>
              ) : (
                editHistory.map((edit, i) => (
                  <div
                    key={edit.id}
                    style={{
                      marginBottom: 12,
                      padding: 12,
                      background: 'var(--bg-tertiary)',
                      borderRadius: 'var(--radius-sm)',
                    }}
                  >
                    <div
                      style={{
                        fontSize: '0.8rem',
                        color: 'var(--text-muted)',
                        marginBottom: 4,
                      }}
                    >
                      {new Date(edit.editedAt).toLocaleString()}
                    </div>
                    <div>{edit.previousContent}</div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      )}

      {/* Read Receipts Modal */}
      {showReadReceipts !== null && (
        <div
          className="modal-overlay"
          onClick={() => setShowReadReceipts(null)}
        >
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Seen by</h3>
              <button
                className="modal-close"
                onClick={() => setShowReadReceipts(null)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              {readReceipts.length === 0 ? (
                <p style={{ color: 'var(--text-muted)' }}>
                  No one has seen this message yet
                </p>
              ) : (
                <div className="user-list">
                  {readReceipts.map(receipt => (
                    <div key={receipt.id} className="user-item">
                      <span className={`user-status ${receipt.user.status}`} />
                      <div className="user-info">
                        <div className="user-name">
                          {receipt.user.displayName}
                        </div>
                        <div className="user-last-active">
                          {formatTime(receipt.readAt)}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Schedule Message Modal */}
      {showScheduleModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowScheduleModal(false)}
        >
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Schedule Message</h3>
              <button
                className="modal-close"
                onClick={() => setShowScheduleModal(false)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <div className="input-group">
                <label>Message</label>
                <textarea
                  className="message-input"
                  value={messageInput}
                  onChange={e => setMessageInput(e.target.value)}
                  placeholder="Enter your message..."
                  style={{ width: '100%', minHeight: 80 }}
                />
              </div>
              <div className="input-group">
                <label>Send at</label>
                <input
                  type="datetime-local"
                  className="input"
                  value={scheduleDate}
                  onChange={e => setScheduleDate(e.target.value)}
                  min={getLocalDateTimeString()}
                />
              </div>
            </div>
            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setShowScheduleModal(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleScheduleMessage}
                disabled={!messageInput.trim() || !scheduleDate}
              >
                Schedule
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Ephemeral Message Modal */}
      {showEphemeralModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowEphemeralModal(false)}
        >
          <div
            className="modal animate-fade-in"
            onClick={e => e.stopPropagation()}
          >
            <div className="modal-header">
              <h3 className="modal-title">Disappearing Message</h3>
              <button
                className="modal-close"
                onClick={() => setShowEphemeralModal(false)}
              >
                ×
              </button>
            </div>
            <div className="modal-body">
              <div className="input-group">
                <label>Message</label>
                <textarea
                  className="message-input"
                  value={messageInput}
                  onChange={e => setMessageInput(e.target.value)}
                  placeholder="Enter your message..."
                  style={{ width: '100%', minHeight: 80 }}
                />
              </div>
              <div className="input-group">
                <label>Delete after (minutes)</label>
                <div style={{ display: 'flex', gap: 8 }}>
                  {[1, 5, 15, 30].map(mins => (
                    <button
                      key={mins}
                      className={`btn btn-sm ${ephemeralMinutes === mins ? 'btn-primary' : 'btn-secondary'}`}
                      onClick={() => setEphemeralMinutes(mins)}
                    >
                      {mins}m
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setShowEphemeralModal(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleSendEphemeral}
                disabled={!messageInput.trim()}
              >
                Send
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
