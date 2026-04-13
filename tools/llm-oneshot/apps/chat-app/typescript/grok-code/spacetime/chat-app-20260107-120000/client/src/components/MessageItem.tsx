import { useState } from 'react';

interface MessageItemProps {
  message: any;
  edits: readonly any[];
  reactions: readonly any[];
  readReceipts: readonly any[];
  users: readonly any[];
  currentUser: any;
  onEdit: (messageId: bigint, newContent: string) => Promise<void>;
  onReact: (messageId: bigint, emoji: string) => Promise<void>;
}

export default function MessageItem({
  message,
  edits,
  reactions,
  readReceipts,
  users,
  currentUser,
  onEdit,
  onReact,
}: MessageItemProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(message.content);
  const [showEditHistory, setShowEditHistory] = useState(false);

  const author = users.find(
    u => u.identity.toHexString() === message.authorId.toHexString()
  );
  const isMyMessage =
    message.authorId.toHexString() === currentUser?.identity.toHexString();

  // Group reactions by emoji
  const reactionGroups = reactions.reduce(
    (acc, reaction) => {
      const emoji = reaction.emoji;
      if (!acc[emoji]) {
        acc[emoji] = { count: 0, users: [], hasMyReaction: false };
      }
      acc[emoji].count++;
      acc[emoji].users.push(reaction.userId.toHexString());
      if (
        reaction.userId.toHexString() === currentUser?.identity.toHexString()
      ) {
        acc[emoji].hasMyReaction = true;
      }
      return acc;
    },
    {} as Record<
      string,
      { count: number; users: string[]; hasMyReaction: boolean }
    >
  );

  const handleEdit = async () => {
    if (editContent.trim() && editContent !== message.content) {
      try {
        await onEdit(message.id, editContent.trim());
        setIsEditing(false);
      } catch (error) {
        console.error('Failed to edit message:', error);
      }
    } else {
      setIsEditing(false);
      setEditContent(message.content);
    }
  };

  const handleReaction = async (emoji: string) => {
    try {
      await onReact(message.id, emoji);
    } catch (error) {
      console.error('Failed to toggle reaction:', error);
    }
  };

  // Format timestamp
  const formatTime = (timestamp: any) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  // Get readers
  const readers = readReceipts
    .filter(r => r.userId.toHexString() !== message.authorId.toHexString())
    .map(r => {
      const user = users.find(
        u => u.identity.toHexString() === r.userId.toHexString()
      );
      return user?.displayName || 'Unknown';
    })
    .filter(Boolean);

  return (
    <div className="message">
      <div className="message-avatar">
        {author?.displayName?.[0]?.toUpperCase() || '?'}
      </div>
      <div className="message-content">
        <div className="message-header">
          <span className="message-author">
            {author?.displayName || 'Unknown'}
          </span>
          <span className="message-time">{formatTime(message.createdAt)}</span>
          {message.isEdited && (
            <span className="edit-indicator">
              (edited)
              {edits.length > 0 && (
                <button
                  onClick={() => setShowEditHistory(!showEditHistory)}
                  className="btn"
                  style={{ marginLeft: '4px', fontSize: '10px' }}
                >
                  {showEditHistory ? '▼' : '▶'}
                </button>
              )}
            </span>
          )}
        </div>

        {isEditing ? (
          <div>
            <input
              type="text"
              value={editContent}
              onChange={e => setEditContent(e.target.value)}
              className="input"
              style={{ marginBottom: '8px', width: '100%' }}
              onKeyDown={e => {
                if (e.key === 'Enter') handleEdit();
                if (e.key === 'Escape') {
                  setIsEditing(false);
                  setEditContent(message.content);
                }
              }}
              autoFocus
            />
            <div>
              <button
                onClick={handleEdit}
                className="btn btn-primary"
                style={{ marginRight: '8px' }}
              >
                Save
              </button>
              <button
                onClick={() => {
                  setIsEditing(false);
                  setEditContent(message.content);
                }}
                className="btn"
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <div className="message-text">{message.content}</div>
        )}

        {/* Edit History */}
        {showEditHistory && edits.length > 0 && (
          <div className="edit-history">
            <div
              style={{
                fontSize: '12px',
                color: 'var(--text-muted)',
                marginBottom: '8px',
              }}
            >
              Edit History:
            </div>
            {[...edits]
              .sort((a: any, b: any) =>
                Number(
                  b.editedAt.microsSinceUnixEpoch -
                    a.editedAt.microsSinceUnixEpoch
                )
              )
              .map((edit: any) => {
                const editor = users.find(
                  u => u.identity.toHexString() === edit.editedBy.toHexString()
                );
                return (
                  <div
                    key={edit.id.toString()}
                    style={{ marginBottom: '4px', fontSize: '12px' }}
                  >
                    <span style={{ color: 'var(--text-muted)' }}>
                      {editor?.displayName || 'Unknown'} edited{' '}
                      {formatTime(edit.editedAt)}:
                    </span>
                    <div
                      style={{
                        marginTop: '2px',
                        padding: '4px',
                        backgroundColor: 'var(--bg-accent)',
                        borderRadius: '2px',
                      }}
                    >
                      {edit.previousContent}
                    </div>
                  </div>
                );
              })}
          </div>
        )}

        {/* Reactions */}
        {Object.keys(reactionGroups).length > 0 && (
          <div className="reactions">
            {Object.entries(reactionGroups).map(([emoji, data]) => (
              <button
                key={emoji}
                onClick={() => handleReaction(emoji)}
                className={`reaction ${(data as any).hasMyReaction ? 'mine' : ''}`}
                title={`${(data as any).users
                  .map((id: string) => {
                    const user = users.find(
                      u => u.identity.toHexString() === id
                    );
                    return user?.displayName || 'Unknown';
                  })
                  .join(', ')} reacted with ${emoji}`}
              >
                {emoji} {(data as any).count}
              </button>
            ))}
          </div>
        )}

        {/* Read Receipts */}
        {readers.length > 0 && (
          <div
            style={{
              fontSize: '11px',
              color: 'var(--text-muted)',
              marginTop: '4px',
            }}
          >
            Seen by {readers.join(', ')}
          </div>
        )}

        {/* Message Actions */}
        {isMyMessage && !isEditing && (
          <div className="message-actions">
            <button
              onClick={() => setIsEditing(true)}
              className="btn"
              title="Edit message"
            >
              Edit
            </button>
            {/* Quick reactions */}
            {['👍', '❤️', '😂', '😮', '😢'].map(emoji => (
              <button
                key={emoji}
                onClick={() => handleReaction(emoji)}
                className="btn"
                title={`React with ${emoji}`}
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
