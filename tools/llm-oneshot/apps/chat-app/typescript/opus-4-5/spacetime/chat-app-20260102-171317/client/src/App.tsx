import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { useTable, Identity } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import './styles.css';

// Reaction emojis
const REACTIONS = ['👍', '❤️', '😂', '😮', '😢', '👎', '🎉', '🔥'];

// Status options
const STATUS_OPTIONS = [
  { value: 'online', label: '🟢 Online' },
  { value: 'away', label: '🌙 Away' },
  { value: 'dnd', label: '⛔ Do Not Disturb' },
  { value: 'invisible', label: '👻 Invisible' },
];

function App() {
  // Connection state
  const [conn, setConn] = useState<DbConnection | null>(window.__db_conn);
  const [myIdentity, setMyIdentity] = useState<Identity | null>(
    window.__my_identity
  );

  // UI state
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [messageInput, setMessageInput] = useState('');
  const [nameInput, setNameInput] = useState('');
  const [newRoomName, setNewRoomName] = useState('');
  const [isPrivateRoom, setIsPrivateRoom] = useState(false);
  const [showScheduleModal, setShowScheduleModal] = useState(false);
  const [scheduleMinutes, setScheduleMinutes] = useState(5);
  const [ephemeralDuration, setEphemeralDuration] = useState(0);
  const [replyingTo, setReplyingTo] = useState<bigint | null>(null);
  const [showThread, setShowThread] = useState<bigint | null>(null);
  const [showEditHistory, setShowEditHistory] = useState<bigint | null>(null);
  const [editingMessage, setEditingMessage] = useState<bigint | null>(null);
  const [editContent, setEditContent] = useState('');
  const [dmUsername, setDmUsername] = useState('');
  const [inviteUsername, setInviteUsername] = useState('');
  const [showInvitations, setShowInvitations] = useState(false);
  const [showMembers, setShowMembers] = useState(false);
  const [showScheduledMessages, setShowScheduledMessages] = useState(false);
  const [activeTab, setActiveTab] = useState<'rooms' | 'dms'>('rooms');

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Table subscriptions
  const [users, usersLoading] = useTable(tables.user);
  const [rooms, roomsLoading] = useTable(tables.room);
  const [roomMembers, membersLoading] = useTable(tables.roomMember);
  const [messages, messagesLoading] = useTable(tables.message);
  const [typingIndicators] = useTable(tables.typingIndicator);
  const [messageReactions] = useTable(tables.messageReaction);
  const [readReceipts] = useTable(tables.readReceipt);
  const [messageEdits] = useTable(tables.messageEdit);
  const [bannedUsers] = useTable(tables.bannedUser);
  const [roomInvitations] = useTable(tables.roomInvitation);
  const [scheduledMessages] = useTable(tables.scheduledMessageView);

  // Poll for connection
  useEffect(() => {
    const interval = setInterval(() => {
      if (window.__db_conn && !conn) setConn(window.__db_conn);
      if (window.__my_identity && !myIdentity)
        setMyIdentity(window.__my_identity);
    }, 100);
    return () => clearInterval(interval);
  }, [conn, myIdentity]);

  // Activity heartbeat
  useEffect(() => {
    if (!conn) return;
    const interval = setInterval(() => {
      conn.reducers.updateActivity({});
    }, 60000); // Every minute
    return () => clearInterval(interval);
  }, [conn]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Current user
  const currentUser = useMemo(() => {
    if (!myIdentity || !users) return null;
    return users.find(
      u => u.identity.toHexString() === myIdentity.toHexString()
    );
  }, [users, myIdentity]);

  // My room memberships
  const myMemberships = useMemo(() => {
    if (!myIdentity || !roomMembers) return [];
    return roomMembers.filter(
      m => m.userId.toHexString() === myIdentity.toHexString()
    );
  }, [roomMembers, myIdentity]);

  // My room IDs
  const myRoomIds = useMemo(() => {
    return new Set(myMemberships.map(m => m.roomId));
  }, [myMemberships]);

  // Visible rooms (public or I'm a member)
  const visibleRooms = useMemo(() => {
    if (!rooms) return [];
    return rooms.filter(r => !r.isPrivate || myRoomIds.has(r.id));
  }, [rooms, myRoomIds]);

  // Public rooms (not DMs)
  const publicRooms = useMemo(() => {
    return visibleRooms.filter(r => !r.isDm && !r.isPrivate);
  }, [visibleRooms]);

  // Private rooms I'm in (not DMs)
  const privateRooms = useMemo(() => {
    return visibleRooms.filter(r => !r.isDm && r.isPrivate);
  }, [visibleRooms]);

  // DMs I'm in
  const dmRooms = useMemo(() => {
    return visibleRooms.filter(r => r.isDm);
  }, [visibleRooms]);

  // Selected room
  const selectedRoom = useMemo(() => {
    if (!selectedRoomId || !rooms) return null;
    return rooms.find(r => r.id === selectedRoomId) || null;
  }, [selectedRoomId, rooms]);

  // Am I a member of selected room?
  const amMember = useMemo(() => {
    return myRoomIds.has(selectedRoomId || 0n);
  }, [myRoomIds, selectedRoomId]);

  // Am I admin of selected room?
  const amAdmin = useMemo(() => {
    if (!selectedRoomId) return false;
    const membership = myMemberships.find(m => m.roomId === selectedRoomId);
    return membership?.isAdmin || false;
  }, [myMemberships, selectedRoomId]);

  // Room messages
  const roomMessages = useMemo(() => {
    if (!selectedRoomId || !messages) return [];
    return messages
      .filter(m => m.roomId === selectedRoomId && !m.parentMessageId)
      .sort((a, b) =>
        Number(
          a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
        )
      );
  }, [messages, selectedRoomId]);

  // Thread messages
  const threadMessages = useMemo(() => {
    if (!showThread || !messages) return [];
    return messages
      .filter(m => m.parentMessageId === showThread)
      .sort((a, b) =>
        Number(
          a.createdAt.microsSinceUnixEpoch - b.createdAt.microsSinceUnixEpoch
        )
      );
  }, [messages, showThread]);

  // Parent message for thread
  const threadParent = useMemo(() => {
    if (!showThread || !messages) return null;
    return messages.find(m => m.id === showThread) || null;
  }, [messages, showThread]);

  // Room members
  const roomMembersList = useMemo(() => {
    if (!selectedRoomId || !roomMembers || !users) return [];
    const membershipMap = new Map(
      roomMembers
        .filter(m => m.roomId === selectedRoomId)
        .map(m => [m.userId.toHexString(), m])
    );
    return users
      .filter(u => membershipMap.has(u.identity.toHexString()))
      .map(u => ({
        user: u,
        membership: membershipMap.get(u.identity.toHexString())!,
      }));
  }, [selectedRoomId, roomMembers, users]);

  // Users currently typing in room
  const usersTyping = useMemo(() => {
    if (!selectedRoomId || !typingIndicators || !users || !myIdentity)
      return [];
    const now = Date.now() * 1000; // Convert to microseconds
    return typingIndicators
      .filter(
        t =>
          t.roomId === selectedRoomId &&
          t.userId.toHexString() !== myIdentity.toHexString() &&
          Number(t.expiresAt.microsSinceUnixEpoch) > now
      )
      .map(t =>
        users.find(u => u.identity.toHexString() === t.userId.toHexString())
      )
      .filter(Boolean);
  }, [typingIndicators, selectedRoomId, users, myIdentity]);

  // Unread counts per room
  const unreadCounts = useMemo(() => {
    const counts = new Map<bigint, number>();
    if (!messages || !myMemberships) return counts;

    for (const membership of myMemberships) {
      const lastReadId = membership.lastReadMessageId;
      const roomMsgs = messages.filter(
        m => m.roomId === membership.roomId && !m.parentMessageId
      );

      if (lastReadId === undefined) {
        counts.set(membership.roomId, roomMsgs.length);
      } else {
        const unread = roomMsgs.filter(m => m.id > lastReadId).length;
        counts.set(membership.roomId, unread);
      }
    }
    return counts;
  }, [messages, myMemberships]);

  // Pending invitations for me
  const myInvitations = useMemo(() => {
    if (!myIdentity || !roomInvitations || !rooms) return [];
    return roomInvitations
      .filter(
        i =>
          i.inviteeId.toHexString() === myIdentity.toHexString() &&
          i.status === 'pending'
      )
      .map(i => ({
        invitation: i,
        room: rooms.find(r => r.id === i.roomId),
        inviter: users?.find(
          u => u.identity.toHexString() === i.inviterId.toHexString()
        ),
      }))
      .filter(i => i.room);
  }, [roomInvitations, rooms, users, myIdentity]);

  // My scheduled messages
  const myScheduledMessages = useMemo(() => {
    if (!myIdentity || !scheduledMessages) return [];
    return scheduledMessages
      .filter(s => s.senderId.toHexString() === myIdentity.toHexString())
      .sort((a, b) =>
        Number(
          a.scheduledFor.microsSinceUnixEpoch -
            b.scheduledFor.microsSinceUnixEpoch
        )
      );
  }, [scheduledMessages, myIdentity]);

  // Get reactions for a message
  const getReactions = useCallback(
    (messageId: bigint) => {
      if (!messageReactions)
        return new Map<
          string,
          { count: number; users: string[]; hasReacted: boolean }
        >();

      const reactions = messageReactions.filter(r => r.messageId === messageId);
      const grouped = new Map<
        string,
        { count: number; users: string[]; hasReacted: boolean }
      >();

      for (const r of reactions) {
        const userName =
          users?.find(u => u.identity.toHexString() === r.userId.toHexString())
            ?.name || 'Unknown';
        const hasReacted = myIdentity
          ? r.userId.toHexString() === myIdentity.toHexString()
          : false;

        if (grouped.has(r.emoji)) {
          const existing = grouped.get(r.emoji)!;
          existing.count++;
          existing.users.push(userName);
          if (hasReacted) existing.hasReacted = true;
        } else {
          grouped.set(r.emoji, { count: 1, users: [userName], hasReacted });
        }
      }
      return grouped;
    },
    [messageReactions, users, myIdentity]
  );

  // Get read receipts for a message
  const getReadReceipts = useCallback(
    (messageId: bigint) => {
      if (!readReceipts || !users || !myIdentity) return [];
      return readReceipts
        .filter(
          r =>
            r.messageId === messageId &&
            r.userId.toHexString() !== myIdentity.toHexString()
        )
        .map(
          r =>
            users.find(u => u.identity.toHexString() === r.userId.toHexString())
              ?.name
        )
        .filter(Boolean) as string[];
    },
    [readReceipts, users, myIdentity]
  );

  // Get edit history for a message
  const getEditHistory = useCallback(
    (messageId: bigint) => {
      if (!messageEdits) return [];
      return messageEdits
        .filter(e => e.messageId === messageId)
        .sort((a, b) =>
          Number(
            b.editedAt.microsSinceUnixEpoch - a.editedAt.microsSinceUnixEpoch
          )
        );
    },
    [messageEdits]
  );

  // Format timestamp
  const formatTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Format relative time
  const formatRelativeTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const now = Date.now();
    const then = Number(timestamp.microsSinceUnixEpoch / 1000n);
    const diff = now - then;

    if (diff < 60000) return 'Just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return new Date(then).toLocaleDateString();
  };

  // Calculate time remaining for ephemeral message
  const getTimeRemaining = (expiresAt: { microsSinceUnixEpoch: bigint }) => {
    const now = BigInt(Date.now() * 1000);
    const remaining = Number(expiresAt.microsSinceUnixEpoch - now) / 1000000;
    if (remaining <= 0) return null;
    if (remaining < 60) return `${Math.ceil(remaining)}s`;
    return `${Math.ceil(remaining / 60)}m`;
  };

  // Handlers
  const handleSetName = () => {
    if (!conn || !nameInput.trim()) return;
    conn.reducers.setName({ name: nameInput.trim() });
    setNameInput('');
  };

  const handleSetStatus = (status: string) => {
    if (!conn) return;
    conn.reducers.setStatus({ status });
  };

  const handleCreateRoom = () => {
    if (!conn || !newRoomName.trim()) return;
    conn.reducers.createRoom({
      name: newRoomName.trim(),
      isPrivate: isPrivateRoom,
    });
    setNewRoomName('');
    setIsPrivateRoom(false);
  };

  const handleJoinRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.joinRoom({ roomId });
  };

  const handleLeaveRoom = (roomId: bigint) => {
    if (!conn) return;
    conn.reducers.leaveRoom({ roomId });
    if (selectedRoomId === roomId) {
      setSelectedRoomId(null);
    }
  };

  const handleStartDm = () => {
    if (!conn || !dmUsername.trim()) return;
    conn.reducers.startDm({ targetUserName: dmUsername.trim() });
    setDmUsername('');
  };

  const handleInviteToRoom = () => {
    if (!conn || !selectedRoomId || !inviteUsername.trim()) return;
    conn.reducers.inviteToRoom({
      roomId: selectedRoomId,
      inviteeName: inviteUsername.trim(),
    });
    setInviteUsername('');
  };

  const handleRespondToInvitation = (invitationId: bigint, accept: boolean) => {
    if (!conn) return;
    conn.reducers.respondToInvitation({ invitationId, accept });
  };

  const handleSendMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;

    if (ephemeralDuration > 0) {
      conn.reducers.sendEphemeralMessage({
        roomId: selectedRoomId,
        content: messageInput.trim(),
        durationSeconds: BigInt(ephemeralDuration),
      });
    } else {
      conn.reducers.sendMessage({
        roomId: selectedRoomId,
        content: messageInput.trim(),
        parentMessageId: replyingTo || undefined,
      });
    }

    setMessageInput('');
    setReplyingTo(null);
    setEphemeralDuration(0);
  };

  const handleScheduleMessage = () => {
    if (!conn || !selectedRoomId || !messageInput.trim()) return;

    const sendAtMicros = BigInt(
      (Date.now() + scheduleMinutes * 60 * 1000) * 1000
    );
    conn.reducers.scheduleMessage({
      roomId: selectedRoomId,
      content: messageInput.trim(),
      sendAtMicros,
    });

    setMessageInput('');
    setShowScheduleModal(false);
  };

  const handleCancelScheduledMessage = (viewId: bigint) => {
    if (!conn) return;
    conn.reducers.cancelScheduledMessage({ viewId });
  };

  const handleEditMessage = (messageId: bigint) => {
    if (!conn || !editContent.trim()) return;
    conn.reducers.editMessage({ messageId, newContent: editContent.trim() });
    setEditingMessage(null);
    setEditContent('');
  };

  const handleDeleteMessage = (messageId: bigint) => {
    if (!conn) return;
    conn.reducers.deleteMessage({ messageId });
  };

  const handleToggleReaction = (messageId: bigint, emoji: string) => {
    if (!conn) return;
    conn.reducers.toggleReaction({ messageId, emoji });
  };

  const handleTyping = () => {
    if (!conn || !selectedRoomId) return;

    conn.reducers.startTyping({ roomId: selectedRoomId });

    // Clear existing timeout
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    // Set new timeout to stop typing
    typingTimeoutRef.current = setTimeout(() => {
      conn.reducers.stopTyping({ roomId: selectedRoomId });
    }, 3000);
  };

  const handleMarkAsRead = useCallback(() => {
    if (!conn || !selectedRoomId || roomMessages.length === 0) return;
    const lastMessage = roomMessages[roomMessages.length - 1];
    conn.reducers.markMessagesRead({
      roomId: selectedRoomId,
      upToMessageId: lastMessage.id,
    });
  }, [conn, selectedRoomId, roomMessages]);

  // Mark as read when viewing room
  useEffect(() => {
    if (selectedRoomId && roomMessages.length > 0) {
      handleMarkAsRead();
    }
  }, [selectedRoomId, roomMessages.length, handleMarkAsRead]);

  const handleKickUser = (userName: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.kickUser({
      roomId: selectedRoomId,
      targetUserName: userName,
    });
  };

  const handleBanUser = (userName: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.banUser({
      roomId: selectedRoomId,
      targetUserName: userName,
      reason: undefined,
    });
  };

  const handlePromoteToAdmin = (userName: string) => {
    if (!conn || !selectedRoomId) return;
    conn.reducers.promoteToAdmin({
      roomId: selectedRoomId,
      targetUserName: userName,
    });
  };

  // Loading state
  if (!conn || usersLoading || roomsLoading) {
    return (
      <div className="loading-container">
        <div className="loading-spinner" />
        <p>Connecting to SpacetimeDB...</p>
      </div>
    );
  }

  // Name setup
  if (!currentUser?.name) {
    return (
      <div className="setup-container">
        <div className="setup-card">
          <h1>Welcome to Chat App</h1>
          <p>Enter your display name to get started</p>
          <div className="setup-form">
            <input
              type="text"
              value={nameInput}
              onChange={e => setNameInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleSetName()}
              placeholder="Your name..."
              maxLength={50}
              autoFocus
            />
            <button onClick={handleSetName} disabled={!nameInput.trim()}>
              Continue
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Render message
  const renderMessage = (msg: (typeof messages)[0], isThread = false) => {
    const sender = users?.find(
      u => u.identity.toHexString() === msg.senderId.toHexString()
    );
    const isMine =
      myIdentity && msg.senderId.toHexString() === myIdentity.toHexString();
    const reactions = getReactions(msg.id);
    const receipts = getReadReceipts(msg.id);
    const editHistory = getEditHistory(msg.id);
    const timeRemaining = msg.expiresAt
      ? getTimeRemaining(msg.expiresAt)
      : null;

    return (
      <div
        key={msg.id.toString()}
        className={`message ${isMine ? 'mine' : ''} ${msg.expiresAt ? 'ephemeral' : ''}`}
      >
        <div className="message-header">
          <span className="message-sender">{sender?.name || 'Unknown'}</span>
          <span className="message-time">{formatTime(msg.createdAt)}</span>
          {msg.isEdited && (
            <span
              className="message-edited"
              onClick={() =>
                setShowEditHistory(showEditHistory === msg.id ? null : msg.id)
              }
            >
              (edited)
            </span>
          )}
          {timeRemaining && (
            <span className="message-ephemeral">🔥 {timeRemaining}</span>
          )}
        </div>

        {editingMessage === msg.id ? (
          <div className="message-edit-form">
            <input
              value={editContent}
              onChange={e => setEditContent(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleEditMessage(msg.id)}
              autoFocus
            />
            <button onClick={() => handleEditMessage(msg.id)}>Save</button>
            <button onClick={() => setEditingMessage(null)}>Cancel</button>
          </div>
        ) : (
          <div className="message-content">{msg.content}</div>
        )}

        {/* Edit History */}
        {showEditHistory === msg.id && editHistory.length > 0 && (
          <div className="edit-history">
            <div className="edit-history-title">Edit History</div>
            {editHistory.map((edit, i) => (
              <div key={i} className="edit-history-item">
                <span className="edit-time">{formatTime(edit.editedAt)}</span>
                <span className="edit-content">{edit.previousContent}</span>
              </div>
            ))}
          </div>
        )}

        {/* Reactions */}
        <div className="message-reactions">
          {[...reactions.entries()].map(([emoji, data]) => (
            <button
              key={emoji}
              className={`reaction-badge ${data.hasReacted ? 'reacted' : ''}`}
              onClick={() => handleToggleReaction(msg.id, emoji)}
              title={data.users.join(', ')}
            >
              {emoji} {data.count}
            </button>
          ))}
        </div>

        {/* Reaction picker */}
        <div className="reaction-picker">
          {REACTIONS.map(emoji => (
            <button
              key={emoji}
              className="reaction-option"
              onClick={() => handleToggleReaction(msg.id, emoji)}
            >
              {emoji}
            </button>
          ))}
        </div>

        {/* Read receipts */}
        {isMine && receipts.length > 0 && (
          <div className="read-receipts">
            Seen by {receipts.slice(0, 3).join(', ')}
            {receipts.length > 3 && ` and ${receipts.length - 3} more`}
          </div>
        )}

        {/* Message actions */}
        <div className="message-actions">
          {!isThread && (
            <button
              onClick={() => {
                setReplyingTo(msg.id);
                setShowThread(null);
              }}
            >
              Reply
            </button>
          )}
          {!isThread && msg.replyCount > 0 && (
            <button
              onClick={() =>
                setShowThread(showThread === msg.id ? null : msg.id)
              }
            >
              {msg.replyCount} {msg.replyCount === 1 ? 'reply' : 'replies'}
            </button>
          )}
          {isMine && (
            <>
              <button
                onClick={() => {
                  setEditingMessage(msg.id);
                  setEditContent(msg.content);
                }}
              >
                Edit
              </button>
              <button onClick={() => handleDeleteMessage(msg.id)}>
                Delete
              </button>
            </>
          )}
          {!isMine && amAdmin && (
            <button onClick={() => handleDeleteMessage(msg.id)}>Delete</button>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="app-container">
      {/* Sidebar */}
      <div className="sidebar">
        {/* User info */}
        <div className="user-panel">
          <div className="user-info">
            <span className="user-name">{currentUser.name}</span>
            <select
              value={currentUser.status}
              onChange={e => handleSetStatus(e.target.value)}
              className="status-select"
            >
              {STATUS_OPTIONS.map(opt => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          {/* Invitations badge */}
          {myInvitations.length > 0 && (
            <button
              className="invitations-badge"
              onClick={() => setShowInvitations(!showInvitations)}
            >
              📬 {myInvitations.length} invitation
              {myInvitations.length > 1 ? 's' : ''}
            </button>
          )}
        </div>

        {/* Invitations panel */}
        {showInvitations && myInvitations.length > 0 && (
          <div className="invitations-panel">
            <h4>Pending Invitations</h4>
            {myInvitations.map(({ invitation, room, inviter }) => (
              <div key={invitation.id.toString()} className="invitation-item">
                <div className="invitation-info">
                  <strong>{room?.name}</strong>
                  <span>from {inviter?.name || 'Unknown'}</span>
                </div>
                <div className="invitation-actions">
                  <button
                    onClick={() =>
                      handleRespondToInvitation(invitation.id, true)
                    }
                  >
                    Accept
                  </button>
                  <button
                    onClick={() =>
                      handleRespondToInvitation(invitation.id, false)
                    }
                  >
                    Decline
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Room/DM tabs */}
        <div className="tabs">
          <button
            className={activeTab === 'rooms' ? 'active' : ''}
            onClick={() => setActiveTab('rooms')}
          >
            Rooms
          </button>
          <button
            className={activeTab === 'dms' ? 'active' : ''}
            onClick={() => setActiveTab('dms')}
          >
            DMs
          </button>
        </div>

        {activeTab === 'rooms' ? (
          <>
            {/* Create room */}
            <div className="create-room">
              <input
                type="text"
                value={newRoomName}
                onChange={e => setNewRoomName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreateRoom()}
                placeholder="New room name..."
                maxLength={100}
              />
              <label className="private-checkbox">
                <input
                  type="checkbox"
                  checked={isPrivateRoom}
                  onChange={e => setIsPrivateRoom(e.target.checked)}
                />
                Private
              </label>
              <button onClick={handleCreateRoom} disabled={!newRoomName.trim()}>
                Create
              </button>
            </div>

            {/* Room list */}
            <div className="room-list">
              <div className="room-section-title">Public Rooms</div>
              {publicRooms.map(room => {
                const isMember = myRoomIds.has(room.id);
                const unread = unreadCounts.get(room.id) || 0;

                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item ${selectedRoomId === room.id ? 'selected' : ''}`}
                    onClick={() => isMember && setSelectedRoomId(room.id)}
                  >
                    <span className="room-name">
                      # {room.name}
                      {unread > 0 && (
                        <span className="unread-badge">{unread}</span>
                      )}
                    </span>
                    {isMember ? (
                      <button
                        className="leave-btn"
                        onClick={e => {
                          e.stopPropagation();
                          handleLeaveRoom(room.id);
                        }}
                      >
                        Leave
                      </button>
                    ) : (
                      <button
                        className="join-btn"
                        onClick={e => {
                          e.stopPropagation();
                          handleJoinRoom(room.id);
                        }}
                      >
                        Join
                      </button>
                    )}
                  </div>
                );
              })}

              {privateRooms.length > 0 && (
                <>
                  <div className="room-section-title">Private Rooms</div>
                  {privateRooms.map(room => {
                    const unread = unreadCounts.get(room.id) || 0;

                    return (
                      <div
                        key={room.id.toString()}
                        className={`room-item ${selectedRoomId === room.id ? 'selected' : ''}`}
                        onClick={() => setSelectedRoomId(room.id)}
                      >
                        <span className="room-name">
                          🔒 {room.name}
                          {unread > 0 && (
                            <span className="unread-badge">{unread}</span>
                          )}
                        </span>
                        <button
                          className="leave-btn"
                          onClick={e => {
                            e.stopPropagation();
                            handleLeaveRoom(room.id);
                          }}
                        >
                          Leave
                        </button>
                      </div>
                    );
                  })}
                </>
              )}
            </div>
          </>
        ) : (
          <>
            {/* Start DM */}
            <div className="create-room">
              <input
                type="text"
                value={dmUsername}
                onChange={e => setDmUsername(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleStartDm()}
                placeholder="Username to DM..."
              />
              <button onClick={handleStartDm} disabled={!dmUsername.trim()}>
                Start DM
              </button>
            </div>

            {/* DM list */}
            <div className="room-list">
              {dmRooms.map(room => {
                const unread = unreadCounts.get(room.id) || 0;
                // Get the other user in the DM
                const otherMember = roomMembers?.find(
                  m =>
                    m.roomId === room.id &&
                    myIdentity &&
                    m.userId.toHexString() !== myIdentity.toHexString()
                );
                const otherUser =
                  otherMember &&
                  users?.find(
                    u =>
                      u.identity.toHexString() ===
                      otherMember.userId.toHexString()
                  );

                return (
                  <div
                    key={room.id.toString()}
                    className={`room-item ${selectedRoomId === room.id ? 'selected' : ''}`}
                    onClick={() => setSelectedRoomId(room.id)}
                  >
                    <span className="room-name">
                      {otherUser?.online ? '🟢' : '⚪'}{' '}
                      {otherUser?.name || 'Unknown'}
                      {unread > 0 && (
                        <span className="unread-badge">{unread}</span>
                      )}
                    </span>
                  </div>
                );
              })}
              {dmRooms.length === 0 && (
                <div className="empty-state">No direct messages yet</div>
              )}
            </div>
          </>
        )}

        {/* Online users */}
        <div className="online-users">
          <div className="online-title">Online Users</div>
          {users
            ?.filter(u => u.online && u.status !== 'invisible')
            .map(u => (
              <div key={u.identity.toHexString()} className="online-user">
                <span className={`status-dot status-${u.status}`} />
                {u.name || 'Anonymous'}
                {u.status === 'dnd' && ' (DND)'}
                {u.status === 'away' && ' (Away)'}
              </div>
            ))}
        </div>
      </div>

      {/* Main chat area */}
      <div className="main-content">
        {selectedRoom ? (
          <>
            {/* Room header */}
            <div className="room-header">
              <div className="room-header-info">
                <h2>
                  {selectedRoom.isDm
                    ? '💬'
                    : selectedRoom.isPrivate
                      ? '🔒'
                      : '#'}{' '}
                  {selectedRoom.name}
                </h2>
                {!selectedRoom.isDm && (
                  <span className="member-count">
                    {roomMembersList.length} member
                    {roomMembersList.length !== 1 ? 's' : ''}
                  </span>
                )}
              </div>
              <div className="room-header-actions">
                {amAdmin && selectedRoom.isPrivate && !selectedRoom.isDm && (
                  <button onClick={() => setShowMembers(!showMembers)}>
                    👥 Manage
                  </button>
                )}
                <button
                  onClick={() =>
                    setShowScheduledMessages(!showScheduledMessages)
                  }
                >
                  ⏰ Scheduled
                </button>
              </div>
            </div>

            {/* Members panel */}
            {showMembers && amAdmin && (
              <div className="members-panel">
                <h4>Room Members</h4>
                <div className="invite-form">
                  <input
                    value={inviteUsername}
                    onChange={e => setInviteUsername(e.target.value)}
                    placeholder="Invite username..."
                  />
                  <button onClick={handleInviteToRoom}>Invite</button>
                </div>
                {roomMembersList.map(({ user, membership }) => (
                  <div
                    key={user.identity.toHexString()}
                    className="member-item"
                  >
                    <span>
                      {user.name}
                      {membership.isAdmin && ' (Admin)'}
                      {user.identity.toHexString() ===
                        selectedRoom.ownerId.toHexString() && ' (Owner)'}
                    </span>
                    {myIdentity &&
                      user.identity.toHexString() !==
                        myIdentity.toHexString() &&
                      user.identity.toHexString() !==
                        selectedRoom.ownerId.toHexString() && (
                        <div className="member-actions">
                          {!membership.isAdmin && (
                            <button
                              onClick={() => handlePromoteToAdmin(user.name!)}
                            >
                              Promote
                            </button>
                          )}
                          <button onClick={() => handleKickUser(user.name!)}>
                            Kick
                          </button>
                          <button onClick={() => handleBanUser(user.name!)}>
                            Ban
                          </button>
                        </div>
                      )}
                  </div>
                ))}
              </div>
            )}

            {/* Scheduled messages panel */}
            {showScheduledMessages && (
              <div className="scheduled-panel">
                <h4>Your Scheduled Messages</h4>
                {myScheduledMessages.length === 0 ? (
                  <div className="empty-state">No scheduled messages</div>
                ) : (
                  myScheduledMessages.map(msg => (
                    <div key={msg.id.toString()} className="scheduled-item">
                      <div className="scheduled-info">
                        <span className="scheduled-time">
                          {new Date(
                            Number(
                              msg.scheduledFor.microsSinceUnixEpoch / 1000n
                            )
                          ).toLocaleString()}
                        </span>
                        <span className="scheduled-room">
                          in{' '}
                          {rooms?.find(r => r.id === msg.roomId)?.name ||
                            'Unknown room'}
                        </span>
                      </div>
                      <div className="scheduled-content">{msg.content}</div>
                      <button
                        onClick={() => handleCancelScheduledMessage(msg.id)}
                      >
                        Cancel
                      </button>
                    </div>
                  ))
                )}
              </div>
            )}

            {/* Messages */}
            <div className="messages-container">
              {messagesLoading ? (
                <div className="loading-messages">Loading messages...</div>
              ) : roomMessages.length === 0 ? (
                <div className="empty-messages">
                  No messages yet. Start the conversation!
                </div>
              ) : (
                roomMessages.map(msg => renderMessage(msg))
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Thread panel */}
            {showThread && threadParent && (
              <div className="thread-panel">
                <div className="thread-header">
                  <h4>Thread</h4>
                  <button onClick={() => setShowThread(null)}>✕</button>
                </div>
                <div className="thread-parent">
                  {renderMessage(threadParent, true)}
                </div>
                <div className="thread-replies">
                  {threadMessages.map(msg => renderMessage(msg, true))}
                </div>
              </div>
            )}

            {/* Typing indicator */}
            {usersTyping.length > 0 && (
              <div className="typing-indicator">
                {usersTyping.length === 1
                  ? `${usersTyping[0]?.name} is typing...`
                  : `${usersTyping.length} users are typing...`}
              </div>
            )}

            {/* Replying indicator */}
            {replyingTo && (
              <div className="replying-to">
                Replying to message...
                <button onClick={() => setReplyingTo(null)}>✕</button>
              </div>
            )}

            {/* Message input */}
            {amMember && (
              <div className="message-input-container">
                <div className="message-input-options">
                  <select
                    value={ephemeralDuration}
                    onChange={e => setEphemeralDuration(Number(e.target.value))}
                    className="ephemeral-select"
                  >
                    <option value={0}>Normal message</option>
                    <option value={60}>🔥 Disappears in 1 min</option>
                    <option value={300}>🔥 Disappears in 5 min</option>
                    <option value={3600}>🔥 Disappears in 1 hour</option>
                  </select>
                </div>
                <div className="message-input-row">
                  <input
                    type="text"
                    value={messageInput}
                    onChange={e => {
                      setMessageInput(e.target.value);
                      handleTyping();
                    }}
                    onKeyDown={e =>
                      e.key === 'Enter' && !e.shiftKey && handleSendMessage()
                    }
                    placeholder={
                      replyingTo ? 'Write a reply...' : 'Write a message...'
                    }
                    maxLength={4000}
                  />
                  <button
                    onClick={handleSendMessage}
                    disabled={!messageInput.trim()}
                  >
                    Send
                  </button>
                  <button
                    onClick={() => setShowScheduleModal(true)}
                    disabled={!messageInput.trim()}
                  >
                    ⏰
                  </button>
                </div>
              </div>
            )}

            {!amMember && (
              <div className="not-member-notice">
                Join this room to send messages
              </div>
            )}
          </>
        ) : (
          <div className="no-room-selected">
            <h2>Welcome to Chat App</h2>
            <p>
              Select a room from the sidebar or create a new one to start
              chatting.
            </p>
          </div>
        )}
      </div>

      {/* Schedule modal */}
      {showScheduleModal && (
        <div
          className="modal-overlay"
          onClick={() => setShowScheduleModal(false)}
        >
          <div className="modal" onClick={e => e.stopPropagation()}>
            <h3>Schedule Message</h3>
            <p>Send this message in:</p>
            <select
              value={scheduleMinutes}
              onChange={e => setScheduleMinutes(Number(e.target.value))}
            >
              <option value={1}>1 minute</option>
              <option value={5}>5 minutes</option>
              <option value={15}>15 minutes</option>
              <option value={30}>30 minutes</option>
              <option value={60}>1 hour</option>
            </select>
            <div className="modal-actions">
              <button onClick={() => setShowScheduleModal(false)}>
                Cancel
              </button>
              <button onClick={handleScheduleMessage}>Schedule</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
