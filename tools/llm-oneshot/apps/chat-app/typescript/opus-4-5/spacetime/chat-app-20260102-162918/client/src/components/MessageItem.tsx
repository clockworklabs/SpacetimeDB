import { useState } from 'react';
import { DbConnection, Message, User, MessageReaction, ReadReceipt, MessageEdit } from '../module_bindings';
import { Identity } from 'spacetimedb/react';
import EditHistoryModal from './EditHistoryModal';

interface MessageItemProps {
  message: Message;
  conn: DbConnection;
  myIdentity: Identity | null;
  users: User[];
  reactions: MessageReaction[];
  readReceipts: ReadReceipt[];
  edits: MessageEdit[];
  replyCount: number;
  onViewThread: () => void;
  isMember: boolean;
}

const REACTION_EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '🎉', '🔥', '👀'];

export default function MessageItem({
  message,
  conn,
  myIdentity,
  users,
  reactions,
  readReceipts,
  edits,
  replyCount,
  onViewThread,
  isMember,
}: MessageItemProps) {
  const [showReactionPicker, setShowReactionPicker] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(message.content);
  const [showEditHistory, setShowEditHistory] = useState(false);

  const sender = users.find(u => u.identity.toHexString() === message.senderId.toHexString());
  const isMyMessage = myIdentity && message.senderId.toHexString() === myIdentity.toHexString();

  const formatTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatDate = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleDateString();
  };

  // Group reactions by emoji
  const reactionGroups = new Map<string, { count: number; users: string[]; hasMyReaction: boolean }>();
  for (const reaction of reactions) {
    const group = reactionGroups.get(reaction.emoji) ?? { count: 0, users: [], hasMyReaction: false };
    group.count++;
    const user = users.find(u => u.identity.toHexString() === reaction.userId.toHexString());
    if (user?.name) group.users.push(user.name);
    if (myIdentity && reaction.userId.toHexString() === myIdentity.toHexString()) {
      group.hasMyReaction = true;
    }
    reactionGroups.set(reaction.emoji, group);
  }

  // Who has seen this message (excluding sender)
  const seenBy = readReceipts
    .filter(r => r.userId.toHexString() !== message.senderId.toHexString())
    .map(r => users.find(u => u.identity.toHexString() === r.userId.toHexString())?.name)
    .filter((name): name is string => !!name);

  const handleReaction = (emoji: string) => {
    conn.reducers.toggleReaction({ messageId: message.id, emoji });
    setShowReactionPicker(false);
  };

  const handleEdit = () => {
    if (editContent.trim() && editContent !== message.content) {
      conn.reducers.editMessage({ messageId: message.id, newContent: editContent.trim() });
    }
    setIsEditing(false);
  };

  const handleDelete = () => {
    if (confirm('Delete this message?')) {
      conn.reducers.deleteMessage({ messageId: message.id });
    }
  };

  // Calculate ephemeral countdown
  const isEphemeral = message.expiresAt != null;
  let ephemeralSecondsLeft = 0;
  if (isEphemeral && message.expiresAt) {
    const now = BigInt(Date.now()) * 1000n;
    ephemeralSecondsLeft = Math.max(0, Number((message.expiresAt.microsSinceUnixEpoch - now) / 1_000_000n));
  }

  return (
    <div className={`message ${isEphemeral ? 'message-ephemeral' : ''}`}>
      <div className="message-avatar">
        {(sender?.name ?? '?')[0].toUpperCase()}
      </div>
      <div className="message-content">
        <div className="message-header">
          <span className="message-author">{sender?.name ?? 'Unknown'}</span>
          <span className="message-timestamp">{formatDate(message.createdAt)} {formatTime(message.createdAt)}</span>
          {message.isEdited && (
            <span
              className="message-edited"
              style={{ cursor: 'pointer' }}
              onClick={() => setShowEditHistory(true)}
              title="Click to view edit history"
            >
              (edited)
            </span>
          )}
        </div>

        {isEditing ? (
          <div style={{ display: 'flex', gap: '8px', marginTop: '4px' }}>
            <input
              type="text"
              className="input"
              value={editContent}
              onChange={e => setEditContent(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Enter') handleEdit();
                if (e.key === 'Escape') setIsEditing(false);
              }}
              autoFocus
            />
            <button className="btn btn-primary btn-small" onClick={handleEdit}>Save</button>
            <button className="btn btn-secondary btn-small" onClick={() => setIsEditing(false)}>Cancel</button>
          </div>
        ) : (
          <div className="message-text">{message.content}</div>
        )}

        {isEphemeral && (
          <div className="ephemeral-indicator">
            ⏳ Disappears in {ephemeralSecondsLeft > 60 ? `${Math.ceil(ephemeralSecondsLeft / 60)}m` : `${ephemeralSecondsLeft}s`}
          </div>
        )}

        {reactionGroups.size > 0 && (
          <div className="reactions-container">
            {[...reactionGroups.entries()].map(([emoji, group]) => (
              <div
                key={emoji}
                className={`reaction ${group.hasMyReaction ? 'my-reaction' : ''}`}
                onClick={() => isMember && handleReaction(emoji)}
                title={group.users.join(', ')}
              >
                {emoji} {group.count}
              </div>
            ))}
          </div>
        )}

        {replyCount > 0 && (
          <div className="thread-indicator" onClick={onViewThread}>
            💬 {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
          </div>
        )}

        {seenBy.length > 0 && isMyMessage && (
          <div className="read-receipts">
            Seen by {seenBy.slice(0, 3).join(', ')}{seenBy.length > 3 ? ` and ${seenBy.length - 3} more` : ''}
          </div>
        )}

        {isMember && (
          <div className="message-actions" style={{ position: 'relative' }}>
            <button
              className="btn-icon btn-small"
              onClick={() => setShowReactionPicker(!showReactionPicker)}
              title="Add reaction"
            >
              😀
            </button>
            <button className="btn-icon btn-small" onClick={onViewThread} title="Reply in thread">
              💬
            </button>
            {isMyMessage && (
              <>
                <button className="btn-icon btn-small" onClick={() => setIsEditing(true)} title="Edit">
                  ✏️
                </button>
                <button className="btn-icon btn-small" onClick={handleDelete} title="Delete">
                  🗑️
                </button>
              </>
            )}

            {showReactionPicker && (
              <div className="reaction-picker">
                {REACTION_EMOJIS.map(emoji => (
                  <button key={emoji} onClick={() => handleReaction(emoji)}>
                    {emoji}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {showEditHistory && (
        <EditHistoryModal
          edits={edits}
          originalContent={message.content}
          onClose={() => setShowEditHistory(false)}
        />
      )}
    </div>
  );
}
