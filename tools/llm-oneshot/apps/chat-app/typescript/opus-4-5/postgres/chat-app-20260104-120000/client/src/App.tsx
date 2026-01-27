import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from './api';
import { connectSocket, disconnectSocket, getSocket } from './socket';
import type {
  User,
  Room,
  Message,
  MessageReaction,
  MemberWithUser,
  MessageWithUser,
  InvitationWithDetails,
  ReceiptWithUser,
  TypingUser,
  MessageEdit,
} from './types';

// =====================
// Utility functions
// =====================

function formatTime(date: string): string {
  return new Date(date).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatDate(date: string): string {
  const d = new Date(date);
  const now = new Date();
  const diff = now.getTime() - d.getTime();

  if (diff < 60000) return 'Just now';
  if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
  return d.toLocaleDateString();
}

function getInitials(name: string): string {
  return name
    .split(' ')
    .map(n => n[0])
    .join('')
    .toUpperCase()
    .slice(0, 2);
}

// =====================
// Login Component
// =====================

function Login({ onLogin }: { onLogin: (user: User, token: string) => void }) {
  const [displayName, setDisplayName] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!displayName.trim()) return;

    setLoading(true);
    setError('');

    try {
      const { user, token } = await api.register(displayName.trim());
      localStorage.setItem('token', token);
      onLogin(user, token);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to register');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="login-screen">
      <div className="login-card">
        <h1 className="login-title">Welcome</h1>
        <p className="login-subtitle">Enter a display name to get started</p>

        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label className="form-label">Display Name</label>
            <input
              type="text"
              className="form-input"
              placeholder="Your name..."
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              maxLength={50}
              autoFocus
            />
          </div>

          {error && (
            <p
              style={{ color: 'var(--danger)', marginBottom: 16, fontSize: 14 }}
            >
              {error}
            </p>
          )}

          <button
            type="submit"
            className="btn btn-primary"
            style={{ width: '100%' }}
            disabled={loading}
          >
            {loading ? 'Joining...' : 'Join Chat'}
          </button>
        </form>
      </div>
    </div>
  );
}

// =====================
// Modal Component
// =====================

function Modal({
  title,
  onClose,
  children,
  footer,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
}) {
  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">{title}</h2>
          <button className="modal-close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-footer">{footer}</div>}
      </div>
    </div>
  );
}

// =====================
// Create Room Modal
// =====================

function CreateRoomModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (room: Room) => void;
}) {
  const [name, setName] = useState('');
  const [isPrivate, setIsPrivate] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setLoading(true);
    setError('');

    try {
      const { room } = await api.createRoom(name.trim(), isPrivate);
      onCreated(room);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create room');
    } finally {
      setLoading(false);
    }
  };

  return (
    <Modal
      title="Create Room"
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={handleSubmit}
            disabled={loading || !name.trim()}
          >
            {loading ? 'Creating...' : 'Create Room'}
          </button>
        </>
      }
    >
      <form onSubmit={handleSubmit}>
        <div className="form-group">
          <label className="form-label">Room Name</label>
          <input
            type="text"
            className="form-input"
            placeholder="general"
            value={name}
            onChange={e => setName(e.target.value)}
            maxLength={100}
            autoFocus
          />
        </div>
        <label className="form-checkbox">
          <input
            type="checkbox"
            checked={isPrivate}
            onChange={e => setIsPrivate(e.target.checked)}
          />
          Make this room private (invite only)
        </label>
        {error && (
          <p style={{ color: 'var(--danger)', marginTop: 12, fontSize: 14 }}>
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}

// =====================
// Create DM Modal
// =====================

function CreateDmModal({
  users,
  currentUserId,
  onClose,
  onCreated,
}: {
  users: User[];
  currentUserId: string;
  onClose: () => void;
  onCreated: (room: Room) => void;
}) {
  const [search, setSearch] = useState('');
  const [loading, setLoading] = useState(false);

  const filteredUsers = users.filter(
    u =>
      u.id !== currentUserId &&
      u.displayName.toLowerCase().includes(search.toLowerCase())
  );

  const handleSelectUser = async (targetUserId: string) => {
    setLoading(true);
    try {
      const { room } = await api.createDm(targetUserId);
      onCreated(room);
      onClose();
    } catch (err) {
      console.error('Failed to create DM:', err);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Modal title="New Direct Message" onClose={onClose}>
      <div className="form-group">
        <input
          type="text"
          className="form-input"
          placeholder="Search users..."
          value={search}
          onChange={e => setSearch(e.target.value)}
          autoFocus
        />
      </div>
      <div style={{ maxHeight: 300, overflowY: 'auto' }}>
        {filteredUsers.length === 0 ? (
          <p
            style={{
              color: 'var(--text-muted)',
              textAlign: 'center',
              padding: 20,
            }}
          >
            No users found
          </p>
        ) : (
          filteredUsers.map(user => (
            <div
              key={user.id}
              className="member-item"
              onClick={() => !loading && handleSelectUser(user.id)}
              style={{ opacity: loading ? 0.5 : 1 }}
            >
              <div className="member-avatar">
                {getInitials(user.displayName)}
                <div className={`member-status ${user.status}`} />
              </div>
              <div className="member-info">
                <div className="member-name">{user.displayName}</div>
              </div>
            </div>
          ))
        )}
      </div>
    </Modal>
  );
}

// =====================
// Invite User Modal
// =====================

function InviteUserModal({
  roomId,
  onClose,
}: {
  roomId: number;
  onClose: () => void;
}) {
  const [username, setUsername] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [success, setSuccess] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username.trim()) return;

    setLoading(true);
    setError('');
    setSuccess(false);

    try {
      await api.inviteUser(roomId, username.trim());
      setSuccess(true);
      setUsername('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to invite user');
    } finally {
      setLoading(false);
    }
  };

  return (
    <Modal
      title="Invite User"
      onClose={onClose}
      footer={
        <button
          className="btn btn-primary"
          onClick={handleSubmit}
          disabled={loading || !username.trim()}
        >
          {loading ? 'Inviting...' : 'Send Invite'}
        </button>
      }
    >
      <form onSubmit={handleSubmit}>
        <div className="form-group">
          <label className="form-label">Username</label>
          <input
            type="text"
            className="form-input"
            placeholder="Enter exact display name..."
            value={username}
            onChange={e => setUsername(e.target.value)}
            autoFocus
          />
        </div>
        {error && (
          <p style={{ color: 'var(--danger)', fontSize: 14 }}>{error}</p>
        )}
        {success && (
          <p style={{ color: 'var(--success)', fontSize: 14 }}>
            Invitation sent!
          </p>
        )}
      </form>
    </Modal>
  );
}

// =====================
// Invitations Modal
// =====================

function InvitationsModal({
  invitations,
  onClose,
  onRespond,
}: {
  invitations: InvitationWithDetails[];
  onClose: () => void;
  onRespond: (invitationId: number, action: 'accept' | 'decline') => void;
}) {
  return (
    <Modal title="Invitations" onClose={onClose}>
      {invitations.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">📭</div>
          <div className="empty-state-text">No pending invitations</div>
        </div>
      ) : (
        <div className="invitation-list">
          {invitations.map(({ invitation, room, inviter }) => (
            <div key={invitation.id} className="invitation-item">
              <div className="invitation-info">
                <div className="invitation-room">{room.name}</div>
                <div className="invitation-from">
                  Invited by {inviter.displayName}
                </div>
              </div>
              <div className="invitation-actions">
                <button
                  className="btn btn-sm btn-secondary"
                  onClick={() => onRespond(invitation.id, 'decline')}
                >
                  Decline
                </button>
                <button
                  className="btn btn-sm btn-primary"
                  onClick={() => onRespond(invitation.id, 'accept')}
                >
                  Accept
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}

// =====================
// Edit History Modal
// =====================

function EditHistoryModal({
  edits,
  onClose,
}: {
  edits: MessageEdit[];
  onClose: () => void;
}) {
  return (
    <Modal title="Edit History" onClose={onClose}>
      {edits.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-text">No edit history</div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {edits.map(edit => (
            <div
              key={edit.id}
              style={{
                padding: 12,
                background: 'var(--bg-tertiary)',
                borderRadius: 'var(--radius-md)',
              }}
            >
              <div
                style={{
                  fontSize: 11,
                  color: 'var(--text-muted)',
                  marginBottom: 4,
                }}
              >
                {formatDate(edit.editedAt)}
              </div>
              <div style={{ fontSize: 14 }}>{edit.previousContent}</div>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}

// =====================
// Scheduled Messages Modal
// =====================

function ScheduledMessagesModal({
  messages,
  onClose,
  onCancel,
}: {
  messages: Message[];
  onClose: () => void;
  onCancel: (messageId: number) => void;
}) {
  return (
    <Modal title="Scheduled Messages" onClose={onClose}>
      {messages.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">📅</div>
          <div className="empty-state-text">No scheduled messages</div>
        </div>
      ) : (
        <div className="scheduled-list">
          {messages.map(message => (
            <div key={message.id} className="scheduled-item">
              <div className="scheduled-time">
                Scheduled for {new Date(message.scheduledFor!).toLocaleString()}
              </div>
              <div className="scheduled-content">{message.content}</div>
              <button
                className="btn btn-sm btn-danger"
                onClick={() => onCancel(message.id)}
              >
                Cancel
              </button>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}

// =====================
// Status Selector
// =====================

function StatusSelector({
  status,
  onChange,
}: {
  status: string;
  onChange: (status: string) => void;
}) {
  const statuses = [
    { value: 'online', label: 'Online' },
    { value: 'away', label: 'Away' },
    { value: 'dnd', label: 'Do Not Disturb' },
    { value: 'invisible', label: 'Invisible' },
  ];

  return (
    <div className="status-select">
      {statuses.map(s => (
        <div
          key={s.value}
          className={`status-option ${status === s.value ? 'selected' : ''}`}
          onClick={() => onChange(s.value)}
        >
          <div className={`status-dot ${s.value}`} />
          {s.label}
        </div>
      ))}
    </div>
  );
}

// =====================
// Message Component
// =====================

function MessageItem({
  message,
  user,
  currentUserId,
  reactions,
  receipts,
  users,
  replyCount,
  replyTo,
  onReact,
  onEdit,
  onViewHistory,
  onReply,
  onViewThread,
}: {
  message: Message;
  user: User;
  currentUserId: string;
  reactions: MessageReaction[];
  receipts: ReceiptWithUser[];
  users: Map<string, User>;
  replyCount?: number;
  replyTo?: MessageWithUser;
  onReact: (messageId: number, emoji: string) => void;
  onEdit: (message: Message) => void;
  onViewHistory: (messageId: number) => void;
  onReply: (message: Message, user: User) => void;
  onViewThread: (messageId: number) => void;
}) {
  const [showEmojiPicker, setShowEmojiPicker] = useState(false);
  const [showReactionUsers, setShowReactionUsers] = useState<string | null>(
    null
  );
  const [timeLeft, setTimeLeft] = useState<number | null>(null);

  const emojis = ['👍', '❤️', '😂', '😮', '😢'];

  // Group reactions by emoji
  const groupedReactions = reactions.reduce(
    (acc, r) => {
      if (!acc[r.emoji]) acc[r.emoji] = [];
      acc[r.emoji].push(r);
      return acc;
    },
    {} as Record<string, MessageReaction[]>
  );

  // Ephemeral countdown
  useEffect(() => {
    if (!message.expiresAt) return;

    const updateTimer = () => {
      const remaining = Math.max(
        0,
        new Date(message.expiresAt!).getTime() - Date.now()
      );
      setTimeLeft(Math.ceil(remaining / 1000));
    };

    updateTimer();
    const interval = setInterval(updateTimer, 1000);
    return () => clearInterval(interval);
  }, [message.expiresAt]);

  const readByUsers = receipts
    .filter(r => r.receipt.userId !== message.userId)
    .map(r => r.user.displayName);

  return (
    <div className={`message ${message.expiresAt ? 'message-ephemeral' : ''}`}>
      {replyTo && (
        <div className="message-reply-indicator">
          ↳ Replying to {replyTo.user.displayName}:{' '}
          {replyTo.message.content.slice(0, 50)}...
        </div>
      )}

      <div className="message-header">
        <span className="message-author">{user.displayName}</span>
        <span className="message-time">{formatTime(message.createdAt)}</span>
        {message.isEdited && (
          <span
            className="message-edited"
            onClick={() => onViewHistory(message.id)}
            style={{ cursor: 'pointer' }}
          >
            (edited)
          </span>
        )}
      </div>

      <div className="message-content">{message.content}</div>

      {message.expiresAt && timeLeft !== null && (
        <div className="message-ephemeral-timer">
          ⏱️ Disappears in {timeLeft}s
        </div>
      )}

      {Object.keys(groupedReactions).length > 0 && (
        <div className="message-reactions">
          {Object.entries(groupedReactions).map(([emoji, reacts]) => (
            <div
              key={emoji}
              className={`reaction-badge ${reacts.some(r => r.userId === currentUserId) ? 'mine' : ''}`}
              onClick={() => onReact(message.id, emoji)}
              onMouseEnter={() => setShowReactionUsers(emoji)}
              onMouseLeave={() => setShowReactionUsers(null)}
            >
              {emoji}
              <span className="reaction-count">{reacts.length}</span>
              {showReactionUsers === emoji && (
                <div className="tooltip-content" style={{ opacity: 1 }}>
                  {reacts
                    .map(r => users.get(r.userId)?.displayName || 'Unknown')
                    .join(', ')}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {readByUsers.length > 0 && (
        <div className="message-read-receipts">
          Seen by {readByUsers.slice(0, 3).join(', ')}
          {readByUsers.length > 3 ? ` +${readByUsers.length - 3}` : ''}
        </div>
      )}

      {replyCount && replyCount > 0 && (
        <div
          className="message-thread-count"
          onClick={() => onViewThread(message.id)}
        >
          💬 {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
        </div>
      )}

      <div className="message-actions">
        <div style={{ position: 'relative' }}>
          <button
            className="message-action-btn"
            onClick={() => setShowEmojiPicker(!showEmojiPicker)}
          >
            😀
          </button>
          {showEmojiPicker && (
            <div className="emoji-picker">
              {emojis.map(emoji => (
                <button
                  key={emoji}
                  className="emoji-btn"
                  onClick={() => {
                    onReact(message.id, emoji);
                    setShowEmojiPicker(false);
                  }}
                >
                  {emoji}
                </button>
              ))}
            </div>
          )}
        </div>
        <button
          className="message-action-btn"
          onClick={() => onReply(message, user)}
        >
          ↩️
        </button>
        {message.userId === currentUserId && (
          <button
            className="message-action-btn"
            onClick={() => onEdit(message)}
          >
            ✏️
          </button>
        )}
      </div>
    </div>
  );
}

// =====================
// Thread Panel
// =====================

function ThreadPanel({
  parentMessage,
  parentUser,
  replies,
  onClose,
  onSendReply,
}: {
  parentMessage: Message;
  parentUser: User;
  replies: MessageWithUser[];
  onClose: () => void;
  onSendReply: (content: string) => void;
}) {
  const [content, setContent] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim()) return;
    onSendReply(content.trim());
    setContent('');
  };

  return (
    <div className="thread-panel">
      <div className="thread-header">
        <span className="thread-title">Thread</span>
        <button className="btn btn-ghost btn-icon" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="thread-messages">
        <div
          className="message"
          style={{
            borderBottom: '1px solid var(--border)',
            paddingBottom: 12,
            marginBottom: 12,
          }}
        >
          <div className="message-header">
            <span className="message-author">{parentUser.displayName}</span>
            <span className="message-time">
              {formatTime(parentMessage.createdAt)}
            </span>
          </div>
          <div className="message-content">{parentMessage.content}</div>
        </div>

        {replies.map(({ message, user }) => (
          <div key={message.id} className="message">
            <div className="message-header">
              <span className="message-author">{user.displayName}</span>
              <span className="message-time">
                {formatTime(message.createdAt)}
              </span>
            </div>
            <div className="message-content">{message.content}</div>
          </div>
        ))}
      </div>

      <div className="message-input-container">
        <form onSubmit={handleSubmit}>
          <div className="message-input-row">
            <textarea
              className="message-input"
              placeholder="Reply in thread..."
              value={content}
              onChange={e => setContent(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  handleSubmit(e);
                }
              }}
              rows={1}
            />
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!content.trim()}
            >
              Send
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// =====================
// Members Panel
// =====================

function MembersPanel({
  members,
  users,
  currentUserId,
  isAdmin,
  onKick,
  onBan,
  onPromote,
}: {
  members: MemberWithUser[];
  users: Map<string, User>;
  currentUserId: string;
  isAdmin: boolean;
  onKick: (userId: string) => void;
  onBan: (userId: string) => void;
  onPromote: (userId: string) => void;
}) {
  const [selectedMember, setSelectedMember] = useState<string | null>(null);

  const onlineMembers = members.filter(m => {
    const u = users.get(m.user.id);
    return u && u.status === 'online';
  });

  const offlineMembers = members.filter(m => {
    const u = users.get(m.user.id);
    return !u || u.status !== 'online';
  });

  const renderMember = (m: MemberWithUser) => {
    const user = users.get(m.user.id) || m.user;
    return (
      <div
        key={m.member.id}
        className="member-item"
        onClick={() =>
          setSelectedMember(selectedMember === m.user.id ? null : m.user.id)
        }
      >
        <div className="member-avatar">
          {getInitials(user.displayName)}
          <div className={`member-status ${user.status || 'offline'}`} />
        </div>
        <div className="member-info">
          <div className="member-name">{user.displayName}</div>
          {user.status !== 'online' && user.lastActiveAt && (
            <div className="member-last-active">
              {formatDate(user.lastActiveAt)}
            </div>
          )}
        </div>
        {m.member.isAdmin && <span className="member-admin-badge">ADMIN</span>}

        {selectedMember === m.user.id &&
          isAdmin &&
          m.user.id !== currentUserId && (
            <div
              style={{
                position: 'absolute',
                right: 8,
                display: 'flex',
                gap: 4,
                zIndex: 10,
              }}
            >
              {!m.member.isAdmin && (
                <button
                  className="btn btn-sm btn-secondary"
                  onClick={e => {
                    e.stopPropagation();
                    onPromote(m.user.id);
                  }}
                >
                  Promote
                </button>
              )}
              <button
                className="btn btn-sm btn-secondary"
                onClick={e => {
                  e.stopPropagation();
                  onKick(m.user.id);
                }}
              >
                Kick
              </button>
              <button
                className="btn btn-sm btn-danger"
                onClick={e => {
                  e.stopPropagation();
                  onBan(m.user.id);
                }}
              >
                Ban
              </button>
            </div>
          )}
      </div>
    );
  };

  return (
    <div className="members-panel">
      <div className="members-panel-header">Members — {members.length}</div>
      <div className="member-list">
        {onlineMembers.length > 0 && (
          <>
            <div
              style={{
                fontSize: 11,
                color: 'var(--text-muted)',
                padding: '8px 12px',
                fontWeight: 600,
              }}
            >
              ONLINE — {onlineMembers.length}
            </div>
            {onlineMembers.map(renderMember)}
          </>
        )}
        {offlineMembers.length > 0 && (
          <>
            <div
              style={{
                fontSize: 11,
                color: 'var(--text-muted)',
                padding: '8px 12px',
                fontWeight: 600,
                marginTop: 8,
              }}
            >
              OFFLINE — {offlineMembers.length}
            </div>
            {offlineMembers.map(renderMember)}
          </>
        )}
      </div>
    </div>
  );
}

// =====================
// Main App Component
// =====================

export default function App() {
  // Auth state
  const [currentUser, setCurrentUser] = useState<User | null>(null);
  const [token, setToken] = useState<string | null>(
    localStorage.getItem('token')
  );
  const [loading, setLoading] = useState(true);

  // App state
  const [rooms, setRooms] = useState<Room[]>([]);
  const [selectedRoom, setSelectedRoom] = useState<Room | null>(null);
  const [messages, setMessages] = useState<MessageWithUser[]>([]);
  const [reactions, setReactions] = useState<MessageReaction[]>([]);
  const [receipts, setReceipts] = useState<ReceiptWithUser[]>([]);
  const [replyCounts, setReplyCounts] = useState<Map<number, number>>(
    new Map()
  );
  const [members, setMembers] = useState<MemberWithUser[]>([]);
  const [users, setUsers] = useState<Map<string, User>>(new Map());
  const [typing, setTyping] = useState<TypingUser[]>([]);
  const [unreadCounts, setUnreadCounts] = useState<Record<number, number>>({});
  const [invitations, setInvitations] = useState<InvitationWithDetails[]>([]);
  const [scheduledMessages, setScheduledMessages] = useState<Message[]>([]);

  // UI state
  const [showCreateRoom, setShowCreateRoom] = useState(false);
  const [showCreateDm, setShowCreateDm] = useState(false);
  const [showInviteUser, setShowInviteUser] = useState(false);
  const [showInvitations, setShowInvitations] = useState(false);
  const [showScheduled, setShowScheduled] = useState(false);
  const [showEditHistory, setShowEditHistory] = useState<number | null>(null);
  const [editHistory, setEditHistory] = useState<MessageEdit[]>([]);
  const [showMembers, setShowMembers] = useState(true);
  const [showStatusMenu, setShowStatusMenu] = useState(false);

  // Message input state
  const [messageContent, setMessageContent] = useState('');
  const [replyTo, setReplyTo] = useState<{
    message: Message;
    user: User;
  } | null>(null);
  const [editingMessage, setEditingMessage] = useState<Message | null>(null);
  const [showScheduleOptions, setShowScheduleOptions] = useState(false);
  const [scheduleDate, setScheduleDate] = useState('');
  const [showEphemeralOptions, setShowEphemeralOptions] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState<number | null>(
    null
  );

  // Thread state
  const [threadMessageId, setThreadMessageId] = useState<number | null>(null);
  const [threadReplies, setThreadReplies] = useState<MessageWithUser[]>([]);

  // Refs
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typingTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activityTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Initialize app
  useEffect(() => {
    const init = async () => {
      if (!token) {
        setLoading(false);
        return;
      }

      try {
        const { user } = await api.getMe();
        setCurrentUser(user);
        connectSocket(token);
        await loadInitialData();
      } catch {
        localStorage.removeItem('token');
        setToken(null);
      } finally {
        setLoading(false);
      }
    };

    init();

    return () => {
      disconnectSocket();
    };
  }, [token]);

  // Load initial data
  const loadInitialData = async () => {
    try {
      const [roomsRes, usersRes, unreadRes, invitationsRes] = await Promise.all(
        [
          api.getRooms(),
          api.getUsers(),
          api.getUnreadCounts(),
          api.getInvitations(),
        ]
      );

      setRooms(roomsRes.rooms);
      setUnreadCounts(unreadRes.unreadCounts);
      setInvitations(invitationsRes.invitations);

      const usersMap = new Map<string, User>();
      usersRes.users.forEach(u => usersMap.set(u.id, u));
      setUsers(usersMap);
    } catch (err) {
      console.error('Failed to load initial data:', err);
    }
  };

  // Socket event handlers
  useEffect(() => {
    const socket = getSocket();
    if (!socket || !currentUser) return;

    // User events
    socket.on('user:online', ({ user }) => {
      setUsers(prev => new Map(prev).set(user.id, user));
    });

    socket.on('user:offline', ({ userId, lastActiveAt }) => {
      setUsers(prev => {
        const updated = new Map(prev);
        const user = updated.get(userId);
        if (user) {
          updated.set(userId, {
            ...user,
            status: 'offline' as any,
            lastActiveAt,
          });
        }
        return updated;
      });
    });

    socket.on('user:status', ({ userId, status, lastActiveAt }) => {
      setUsers(prev => {
        const updated = new Map(prev);
        const user = updated.get(userId);
        if (user) {
          updated.set(userId, { ...user, status, lastActiveAt });
        }
        return updated;
      });
    });

    socket.on('user:updated', ({ user }) => {
      setUsers(prev => new Map(prev).set(user.id, user));
      if (user.id === currentUser.id) {
        setCurrentUser(user);
      }
    });

    // Room events
    socket.on('room:created', ({ room }) => {
      setRooms(prev => {
        if (prev.some(r => r.id === room.id)) return prev;
        return [...prev, room];
      });
    });

    socket.on('room:member:joined', ({ roomId, user }) => {
      setUsers(prev => new Map(prev).set(user.id, user));
      if (selectedRoom?.id === roomId) {
        loadRoomMembers(roomId);
      }
    });

    socket.on('room:member:left', ({ userId }) => {
      setMembers(prev => prev.filter(m => m.user.id !== userId));
    });

    socket.on('room:member:kicked', ({ userId }) => {
      setMembers(prev => prev.filter(m => m.user.id !== userId));
    });

    socket.on('room:member:banned', ({ userId }) => {
      setMembers(prev => prev.filter(m => m.user.id !== userId));
    });

    socket.on('room:member:promoted', ({ userId }) => {
      setMembers(prev =>
        prev.map(m =>
          m.user.id === userId
            ? { ...m, member: { ...m.member, isAdmin: true } }
            : m
        )
      );
    });

    socket.on('room:kicked', ({ roomId }) => {
      setRooms(prev => prev.filter(r => r.id !== roomId));
      if (selectedRoom?.id === roomId) {
        setSelectedRoom(null);
        setMessages([]);
      }
    });

    socket.on('room:banned', ({ roomId }) => {
      setRooms(prev => prev.filter(r => r.id !== roomId));
      if (selectedRoom?.id === roomId) {
        setSelectedRoom(null);
        setMessages([]);
      }
    });

    // Message events
    socket.on('message:created', ({ message, user }) => {
      setUsers(prev => new Map(prev).set(user.id, user));

      if (message.roomId === selectedRoom?.id) {
        setMessages(prev => [...prev, { message, user }]);
        scrollToBottom();
      } else {
        setUnreadCounts(prev => ({
          ...prev,
          [message.roomId]: (prev[message.roomId] || 0) + 1,
        }));
      }
    });

    socket.on('message:updated', ({ message, user }) => {
      setMessages(prev =>
        prev.map(m => (m.message.id === message.id ? { message, user } : m))
      );
    });

    socket.on('message:deleted', ({ messageId, roomId }) => {
      if (roomId === selectedRoom?.id) {
        setMessages(prev => prev.filter(m => m.message.id !== messageId));
      }
    });

    socket.on('thread:reply', ({ parentId, message, user }) => {
      setReplyCounts(prev => {
        const updated = new Map(prev);
        updated.set(parentId, (updated.get(parentId) || 0) + 1);
        return updated;
      });

      if (threadMessageId === parentId) {
        setThreadReplies(prev => [...prev, { message, user }]);
      }
    });

    // Reaction events
    socket.on('reaction:added', ({ messageId, userId, emoji, user }) => {
      setUsers(prev => new Map(prev).set(user.id, user));
      setReactions(prev => [
        ...prev,
        {
          id: Date.now(),
          messageId,
          userId,
          emoji,
          createdAt: new Date().toISOString(),
        },
      ]);
    });

    socket.on('reaction:removed', ({ messageId, userId, emoji }) => {
      setReactions(prev =>
        prev.filter(
          r =>
            !(
              r.messageId === messageId &&
              r.userId === userId &&
              r.emoji === emoji
            )
        )
      );
    });

    // Read receipt events
    socket.on('messages:read', ({ roomId, userId, messageIds, user }) => {
      if (roomId === selectedRoom?.id) {
        setReceipts(prev => {
          const newReceipts = messageIds
            .filter(
              (id: number) =>
                !prev.some(
                  r => r.receipt.messageId === id && r.receipt.userId === userId
                )
            )
            .map((id: number) => ({
              receipt: {
                id: Date.now(),
                messageId: id,
                userId,
                readAt: new Date().toISOString(),
              },
              user,
            }));
          return [...prev, ...newReceipts];
        });
      }
    });

    // Typing events
    socket.on('typing:start', ({ roomId, userId, user }) => {
      if (roomId === selectedRoom?.id && userId !== currentUser.id) {
        setTyping(prev => {
          if (prev.some(t => t.userId === userId)) return prev;
          return [...prev, { roomId, userId, user }];
        });
      }
    });

    socket.on('typing:stop', ({ userId }) => {
      setTyping(prev => prev.filter(t => t.userId !== userId));
    });

    // Invitation events
    socket.on('invitation:received', ({ invitation, room, inviter }) => {
      setInvitations(prev => [...prev, { invitation, room, inviter }]);
    });

    return () => {
      socket.off('user:online');
      socket.off('user:offline');
      socket.off('user:status');
      socket.off('user:updated');
      socket.off('room:created');
      socket.off('room:member:joined');
      socket.off('room:member:left');
      socket.off('room:member:kicked');
      socket.off('room:member:banned');
      socket.off('room:member:promoted');
      socket.off('room:kicked');
      socket.off('room:banned');
      socket.off('message:created');
      socket.off('message:updated');
      socket.off('message:deleted');
      socket.off('thread:reply');
      socket.off('reaction:added');
      socket.off('reaction:removed');
      socket.off('messages:read');
      socket.off('typing:start');
      socket.off('typing:stop');
      socket.off('invitation:received');
    };
  }, [currentUser, selectedRoom, threadMessageId]);

  // Activity tracking for auto-away
  useEffect(() => {
    const socket = getSocket();
    if (!socket) return;

    const handleActivity = () => {
      if (activityTimeoutRef.current) {
        clearTimeout(activityTimeoutRef.current);
      }
      socket.emit('activity');
      activityTimeoutRef.current = setTimeout(() => {
        // Activity timeout handled by server
      }, 60000);
    };

    window.addEventListener('mousemove', handleActivity);
    window.addEventListener('keydown', handleActivity);

    return () => {
      window.removeEventListener('mousemove', handleActivity);
      window.removeEventListener('keydown', handleActivity);
    };
  }, []);

  // Load room data when selected
  const loadRoomData = useCallback(async (room: Room) => {
    try {
      const socket = getSocket();
      if (socket) {
        socket.emit('room:join', room.id);
      }

      const [messagesRes, membersRes, scheduledRes] = await Promise.all([
        api.getMessages(room.id),
        api.getMembers(room.id),
        api.getScheduled(room.id),
      ]);

      setMessages(messagesRes.messages);
      setReactions(messagesRes.reactions);
      setReceipts(messagesRes.receipts);
      setMembers(membersRes.members);
      setScheduledMessages(scheduledRes.scheduled);

      // Update reply counts
      const counts = new Map<number, number>();
      messagesRes.replyCounts.forEach((rc: any) => {
        if (rc.replyToId) counts.set(rc.replyToId, rc.count);
      });
      setReplyCounts(counts);

      // Update users
      setUsers(prev => {
        const updated = new Map(prev);
        membersRes.members.forEach((m: MemberWithUser) =>
          updated.set(m.user.id, m.user)
        );
        messagesRes.messages.forEach((m: MessageWithUser) =>
          updated.set(m.user.id, m.user)
        );
        return updated;
      });

      // Mark all as read
      const messageIds = messagesRes.messages.map(
        (m: MessageWithUser) => m.message.id
      );
      if (messageIds.length > 0) {
        await api.markRead(room.id, messageIds);
        setUnreadCounts(prev => ({ ...prev, [room.id]: 0 }));
      }

      scrollToBottom();
    } catch (err) {
      console.error('Failed to load room data:', err);
    }
  }, []);

  const loadRoomMembers = async (roomId: number) => {
    try {
      const { members } = await api.getMembers(roomId);
      setMembers(members);
    } catch (err) {
      console.error('Failed to load members:', err);
    }
  };

  const scrollToBottom = () => {
    setTimeout(() => {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }, 100);
  };

  // Handle room selection
  const handleSelectRoom = (room: Room) => {
    if (selectedRoom?.id === room.id) return;

    const socket = getSocket();
    if (socket && selectedRoom) {
      socket.emit('room:leave', selectedRoom.id);
    }

    setSelectedRoom(room);
    setMessages([]);
    setMembers([]);
    setReplyTo(null);
    setEditingMessage(null);
    setThreadMessageId(null);
    setTyping([]);
    loadRoomData(room);
  };

  // Handle login
  const handleLogin = (user: User, token: string) => {
    setCurrentUser(user);
    setToken(token);
    connectSocket(token);
    loadInitialData();
  };

  // Handle logout
  const handleLogout = () => {
    localStorage.removeItem('token');
    disconnectSocket();
    setToken(null);
    setCurrentUser(null);
    setRooms([]);
    setSelectedRoom(null);
  };

  // Handle send message
  const handleSendMessage = async () => {
    if (!messageContent.trim() || !selectedRoom) return;

    try {
      if (editingMessage) {
        await api.editMessage(editingMessage.id, messageContent.trim());
        setEditingMessage(null);
      } else {
        await api.sendMessage(selectedRoom.id, messageContent.trim(), {
          replyToId: replyTo?.message.id,
          scheduledFor: scheduleDate || undefined,
          expiresIn: ephemeralDuration || undefined,
        });

        if (scheduleDate) {
          const { scheduled } = await api.getScheduled(selectedRoom.id);
          setScheduledMessages(scheduled);
        }
      }

      setMessageContent('');
      setReplyTo(null);
      setScheduleDate('');
      setEphemeralDuration(null);
      setShowScheduleOptions(false);
      setShowEphemeralOptions(false);
    } catch (err) {
      console.error('Failed to send message:', err);
    }
  };

  // Handle typing
  const handleTyping = () => {
    const socket = getSocket();
    if (!socket || !selectedRoom) return;

    socket.emit('typing:start', selectedRoom.id);

    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    typingTimeoutRef.current = setTimeout(() => {
      socket.emit('typing:stop', selectedRoom.id);
    }, 3000);
  };

  // Handle reaction
  const handleReaction = async (messageId: number, emoji: string) => {
    try {
      await api.toggleReaction(messageId, emoji);
    } catch (err) {
      console.error('Failed to toggle reaction:', err);
    }
  };

  // Handle view edit history
  const handleViewHistory = async (messageId: number) => {
    try {
      const { edits } = await api.getMessageHistory(messageId);
      setEditHistory(edits);
      setShowEditHistory(messageId);
    } catch (err) {
      console.error('Failed to load edit history:', err);
    }
  };

  // Handle view thread
  const handleViewThread = async (messageId: number) => {
    try {
      const { replies } = await api.getReplies(messageId);
      setThreadReplies(replies);
      setThreadMessageId(messageId);
    } catch (err) {
      console.error('Failed to load thread:', err);
    }
  };

  // Handle send thread reply
  const handleSendThreadReply = async (content: string) => {
    if (!selectedRoom || !threadMessageId) return;

    try {
      await api.sendMessage(selectedRoom.id, content, {
        replyToId: threadMessageId,
      });
    } catch (err) {
      console.error('Failed to send reply:', err);
    }
  };

  // Handle leave room
  const handleLeaveRoom = async () => {
    if (!selectedRoom) return;

    try {
      await api.leaveRoom(selectedRoom.id);
      setRooms(prev => prev.filter(r => r.id !== selectedRoom.id));
      setSelectedRoom(null);
      setMessages([]);
    } catch (err) {
      console.error('Failed to leave room:', err);
    }
  };

  // Handle invitation response
  const handleInvitationResponse = async (
    invitationId: number,
    action: 'accept' | 'decline'
  ) => {
    try {
      const result = await api.respondToInvitation(invitationId, action);
      setInvitations(prev =>
        prev.filter(i => i.invitation.id !== invitationId)
      );

      if (action === 'accept' && result.room) {
        setRooms(prev => [...prev, result.room]);
      }
    } catch (err) {
      console.error('Failed to respond to invitation:', err);
    }
  };

  // Handle status change
  const handleStatusChange = async (status: string) => {
    try {
      const { user } = await api.updateStatus(status);
      setCurrentUser(user);
      setShowStatusMenu(false);
    } catch (err) {
      console.error('Failed to update status:', err);
    }
  };

  // Handle kick/ban/promote
  const handleKick = async (userId: string) => {
    if (!selectedRoom) return;
    try {
      await api.kickUser(selectedRoom.id, userId);
    } catch (err) {
      console.error('Failed to kick user:', err);
    }
  };

  const handleBan = async (userId: string) => {
    if (!selectedRoom) return;
    try {
      await api.banUser(selectedRoom.id, userId);
    } catch (err) {
      console.error('Failed to ban user:', err);
    }
  };

  const handlePromote = async (userId: string) => {
    if (!selectedRoom) return;
    try {
      await api.promoteUser(selectedRoom.id, userId);
    } catch (err) {
      console.error('Failed to promote user:', err);
    }
  };

  // Handle cancel scheduled message
  const handleCancelScheduled = async (messageId: number) => {
    try {
      await api.cancelScheduled(messageId);
      setScheduledMessages(prev => prev.filter(m => m.id !== messageId));
    } catch (err) {
      console.error('Failed to cancel scheduled message:', err);
    }
  };

  // Loading state
  if (loading) {
    return (
      <div className="login-screen">
        <div className="loading">
          <div className="loading-spinner" />
        </div>
      </div>
    );
  }

  // Login screen
  if (!currentUser) {
    return <Login onLogin={handleLogin} />;
  }

  // Get current user's membership
  const currentMembership = members.find(m => m.user.id === currentUser.id);
  const isAdmin = currentMembership?.member.isAdmin || false;
  const isMember = !!currentMembership;

  // Get thread parent message
  const threadParent = threadMessageId
    ? messages.find(m => m.message.id === threadMessageId)
    : null;

  // Typing text
  const typingText =
    typing.length === 0
      ? null
      : typing.length === 1
        ? `${typing[0].user?.displayName || 'Someone'} is typing...`
        : typing.length === 2
          ? `${typing[0].user?.displayName || 'Someone'} and ${typing[1].user?.displayName || 'someone'} are typing...`
          : 'Multiple users are typing...';

  // Get local datetime for min value
  const now = new Date();
  const minDateTime = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}T${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;

  // Separate rooms by type
  const publicRooms = rooms.filter(r => !r.isPrivate && !r.isDm);
  const privateRooms = rooms.filter(r => r.isPrivate && !r.isDm);
  const dmRooms = rooms.filter(r => r.isDm);

  return (
    <div className="app">
      {/* Sidebar */}
      <div className="sidebar">
        <div className="sidebar-header">
          <span className="sidebar-title">Chat</span>
          <div style={{ display: 'flex', gap: 8 }}>
            {invitations.length > 0 && (
              <button
                className="btn btn-ghost btn-icon"
                onClick={() => setShowInvitations(true)}
                style={{ position: 'relative' }}
              >
                📬
                <span
                  style={{
                    position: 'absolute',
                    top: 0,
                    right: 0,
                    background: 'var(--danger)',
                    color: 'white',
                    fontSize: 10,
                    padding: '2px 5px',
                    borderRadius: 'var(--radius-full)',
                  }}
                >
                  {invitations.length}
                </span>
              </button>
            )}
          </div>
        </div>

        <div className="sidebar-section">
          {/* Public Rooms */}
          <div className="sidebar-section-title">
            Rooms
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => setShowCreateRoom(true)}
            >
              +
            </button>
          </div>
          <div className="room-list">
            {publicRooms.map(room => (
              <div
                key={room.id}
                className={`room-item ${selectedRoom?.id === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room)}
              >
                <span className="room-item-name"># {room.name}</span>
                {unreadCounts[room.id] > 0 && (
                  <span className="room-item-badge">
                    {unreadCounts[room.id]}
                  </span>
                )}
              </div>
            ))}
          </div>

          {/* Private Rooms */}
          {privateRooms.length > 0 && (
            <>
              <div className="sidebar-section-title" style={{ marginTop: 16 }}>
                Private Rooms
              </div>
              <div className="room-list">
                {privateRooms.map(room => (
                  <div
                    key={room.id}
                    className={`room-item ${selectedRoom?.id === room.id ? 'active' : ''}`}
                    onClick={() => handleSelectRoom(room)}
                  >
                    <span className="room-item-private">🔒</span>
                    <span className="room-item-name">{room.name}</span>
                    {unreadCounts[room.id] > 0 && (
                      <span className="room-item-badge">
                        {unreadCounts[room.id]}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </>
          )}

          {/* DMs */}
          <div className="sidebar-section-title" style={{ marginTop: 16 }}>
            Direct Messages
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => setShowCreateDm(true)}
            >
              +
            </button>
          </div>
          <div className="room-list">
            {dmRooms.map(room => (
              <div
                key={room.id}
                className={`room-item ${selectedRoom?.id === room.id ? 'active' : ''}`}
                onClick={() => handleSelectRoom(room)}
              >
                <span className="room-item-name">{room.name}</span>
                {unreadCounts[room.id] > 0 && (
                  <span className="room-item-badge">
                    {unreadCounts[room.id]}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* User panel */}
        <div className="user-panel">
          <div className="user-profile">
            <div className="user-avatar">
              {getInitials(currentUser.displayName)}
            </div>
            <div className="user-info">
              <div className="user-name">{currentUser.displayName}</div>
              <div
                className="user-status-text"
                style={{ cursor: 'pointer' }}
                onClick={() => setShowStatusMenu(!showStatusMenu)}
              >
                {currentUser.status} ▾
              </div>
            </div>
            <button
              className="btn btn-ghost btn-icon"
              onClick={handleLogout}
              title="Logout"
            >
              ⏻
            </button>
          </div>

          {showStatusMenu && (
            <div style={{ marginTop: 12 }}>
              <StatusSelector
                status={currentUser.status}
                onChange={handleStatusChange}
              />
            </div>
          )}
        </div>
      </div>

      {/* Main content */}
      <div className="main">
        {selectedRoom ? (
          <>
            <div className="main-header">
              <div className="main-header-title">
                {selectedRoom.isPrivate && !selectedRoom.isDm && '🔒 '}
                {selectedRoom.isDm ? '' : '# '}
                {selectedRoom.name}
              </div>
              <div className="main-header-actions">
                {selectedRoom.isPrivate && isAdmin && (
                  <button
                    className="btn btn-secondary btn-sm"
                    onClick={() => setShowInviteUser(true)}
                  >
                    Invite
                  </button>
                )}
                {scheduledMessages.length > 0 && (
                  <button
                    className="btn btn-secondary btn-sm"
                    onClick={() => setShowScheduled(true)}
                  >
                    📅 {scheduledMessages.length}
                  </button>
                )}
                <button
                  className={`btn btn-secondary btn-sm ${showMembers ? 'active' : ''}`}
                  onClick={() => setShowMembers(!showMembers)}
                >
                  👥
                </button>
                {isMember && !selectedRoom.isDm && (
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={handleLeaveRoom}
                  >
                    Leave
                  </button>
                )}
              </div>
            </div>

            {/* Messages */}
            <div className="messages-container">
              {messages.length === 0 ? (
                <div className="empty-state">
                  <div className="empty-state-icon">💬</div>
                  <div className="empty-state-text">No messages yet</div>
                  <div className="empty-state-subtext">
                    Be the first to say something!
                  </div>
                </div>
              ) : (
                messages.map(({ message, user }) => {
                  const replyToMessage = message.replyToId
                    ? messages.find(m => m.message.id === message.replyToId)
                    : undefined;

                  return (
                    <MessageItem
                      key={message.id}
                      message={message}
                      user={user}
                      currentUserId={currentUser.id}
                      reactions={reactions.filter(
                        r => r.messageId === message.id
                      )}
                      receipts={receipts.filter(
                        r => r.receipt.messageId === message.id
                      )}
                      users={users}
                      replyCount={replyCounts.get(message.id)}
                      replyTo={replyToMessage}
                      onReact={handleReaction}
                      onEdit={m => {
                        setEditingMessage(m);
                        setMessageContent(m.content);
                      }}
                      onViewHistory={handleViewHistory}
                      onReply={(m, u) => setReplyTo({ message: m, user: u })}
                      onViewThread={handleViewThread}
                    />
                  );
                })
              )}
              <div ref={messagesEndRef} />
            </div>

            {/* Typing indicator */}
            {typingText && <div className="typing-indicator">{typingText}</div>}

            {/* Message input */}
            {(isMember || !selectedRoom.isPrivate) && (
              <div className="message-input-container">
                {replyTo && (
                  <div className="reply-preview">
                    <span>Replying to {replyTo.user.displayName}</span>
                    <button
                      className="reply-preview-close"
                      onClick={() => setReplyTo(null)}
                    >
                      ×
                    </button>
                  </div>
                )}

                {editingMessage && (
                  <div className="reply-preview">
                    <span>Editing message</span>
                    <button
                      className="reply-preview-close"
                      onClick={() => {
                        setEditingMessage(null);
                        setMessageContent('');
                      }}
                    >
                      ×
                    </button>
                  </div>
                )}

                <div className="message-input-options">
                  <button
                    className={`input-option-btn ${showScheduleOptions ? 'active' : ''}`}
                    onClick={() => {
                      setShowScheduleOptions(!showScheduleOptions);
                      setShowEphemeralOptions(false);
                    }}
                    title="Schedule message"
                  >
                    📅
                  </button>
                  <button
                    className={`input-option-btn ${showEphemeralOptions ? 'active' : ''}`}
                    onClick={() => {
                      setShowEphemeralOptions(!showEphemeralOptions);
                      setShowScheduleOptions(false);
                    }}
                    title="Ephemeral message"
                  >
                    ⏱️
                  </button>
                </div>

                {showScheduleOptions && (
                  <div style={{ marginBottom: 12 }}>
                    <input
                      type="datetime-local"
                      className="form-input"
                      value={scheduleDate}
                      onChange={e => setScheduleDate(e.target.value)}
                      min={minDateTime}
                      style={{ width: 'auto' }}
                    />
                    {scheduleDate && (
                      <button
                        className="btn btn-ghost btn-sm"
                        onClick={() => setScheduleDate('')}
                        style={{ marginLeft: 8 }}
                      >
                        Clear
                      </button>
                    )}
                  </div>
                )}

                {showEphemeralOptions && (
                  <div style={{ marginBottom: 12, display: 'flex', gap: 8 }}>
                    {[
                      { label: '1m', value: 60 },
                      { label: '5m', value: 300 },
                      { label: '15m', value: 900 },
                    ].map(opt => (
                      <button
                        key={opt.value}
                        className={`btn btn-sm ${ephemeralDuration === opt.value ? 'btn-primary' : 'btn-secondary'}`}
                        onClick={() =>
                          setEphemeralDuration(
                            ephemeralDuration === opt.value ? null : opt.value
                          )
                        }
                      >
                        {opt.label}
                      </button>
                    ))}
                    {ephemeralDuration && (
                      <span
                        style={{
                          fontSize: 12,
                          color: 'var(--warning)',
                          alignSelf: 'center',
                        }}
                      >
                        Message will disappear after {ephemeralDuration / 60}{' '}
                        minute(s)
                      </span>
                    )}
                  </div>
                )}

                <div className="message-input-row">
                  <textarea
                    className="message-input"
                    placeholder={
                      scheduleDate
                        ? 'Schedule a message...'
                        : ephemeralDuration
                          ? 'Send a disappearing message...'
                          : 'Type a message...'
                    }
                    value={messageContent}
                    onChange={e => {
                      setMessageContent(e.target.value);
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
                    className="btn btn-primary"
                    onClick={handleSendMessage}
                    disabled={!messageContent.trim()}
                  >
                    {editingMessage
                      ? 'Save'
                      : scheduleDate
                        ? 'Schedule'
                        : 'Send'}
                  </button>
                </div>
              </div>
            )}

            {!isMember && selectedRoom.isPrivate && (
              <div
                style={{
                  padding: 20,
                  textAlign: 'center',
                  color: 'var(--text-muted)',
                }}
              >
                You are not a member of this room
              </div>
            )}
          </>
        ) : (
          <div className="empty-state" style={{ flex: 1 }}>
            <div className="empty-state-icon">👋</div>
            <div className="empty-state-text">Welcome to Chat!</div>
            <div className="empty-state-subtext">
              Select a room or start a conversation
            </div>
          </div>
        )}
      </div>

      {/* Members panel */}
      {selectedRoom && showMembers && (
        <MembersPanel
          members={members}
          users={users}
          currentUserId={currentUser.id}
          isAdmin={isAdmin}
          onKick={handleKick}
          onBan={handleBan}
          onPromote={handlePromote}
        />
      )}

      {/* Thread panel */}
      {threadMessageId && threadParent && (
        <ThreadPanel
          parentMessage={threadParent.message}
          parentUser={threadParent.user}
          replies={threadReplies}
          onClose={() => setThreadMessageId(null)}
          onSendReply={handleSendThreadReply}
        />
      )}

      {/* Modals */}
      {showCreateRoom && (
        <CreateRoomModal
          onClose={() => setShowCreateRoom(false)}
          onCreated={room => {
            setRooms(prev => [...prev, room]);
            handleSelectRoom(room);
          }}
        />
      )}

      {showCreateDm && (
        <CreateDmModal
          users={Array.from(users.values())}
          currentUserId={currentUser.id}
          onClose={() => setShowCreateDm(false)}
          onCreated={room => {
            setRooms(prev => [...prev, room]);
            handleSelectRoom(room);
          }}
        />
      )}

      {showInviteUser && selectedRoom && (
        <InviteUserModal
          roomId={selectedRoom.id}
          onClose={() => setShowInviteUser(false)}
        />
      )}

      {showInvitations && (
        <InvitationsModal
          invitations={invitations}
          onClose={() => setShowInvitations(false)}
          onRespond={handleInvitationResponse}
        />
      )}

      {showEditHistory !== null && (
        <EditHistoryModal
          edits={editHistory}
          onClose={() => setShowEditHistory(null)}
        />
      )}

      {showScheduled && (
        <ScheduledMessagesModal
          messages={scheduledMessages}
          onClose={() => setShowScheduled(false)}
          onCancel={handleCancelScheduled}
        />
      )}
    </div>
  );
}
