import React, { useState } from 'react';
import { Message, User, Reaction } from '../types';
import { formatDistanceToNow } from 'date-fns';

interface MessageListProps {
  messages: Message[];
  currentUser: User;
  onEditMessage: (messageId: string, content: string) => void;
  onAddReaction: (messageId: string, emoji: string) => void;
  onRemoveReaction: (messageId: string, emoji: string) => void;
}

function MessageList({
  messages,
  currentUser,
  onEditMessage,
  onAddReaction,
  onRemoveReaction,
}: MessageListProps) {
  const [editingMessageId, setEditingMessageId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState('');

  const handleEditStart = (message: Message) => {
    setEditingMessageId(message.id);
    setEditContent(message.content);
  };

  const handleEditSave = () => {
    if (editingMessageId && editContent.trim()) {
      onEditMessage(editingMessageId, editContent.trim());
      setEditingMessageId(null);
      setEditContent('');
    }
  };

  const handleEditCancel = () => {
    setEditingMessageId(null);
    setEditContent('');
  };

  const handleReactionClick = (
    messageId: string,
    emoji: string,
    hasReacted: boolean
  ) => {
    if (hasReacted) {
      onRemoveReaction(messageId, emoji);
    } else {
      onAddReaction(messageId, emoji);
    }
  };

  const availableEmojis = ['👍', '❤️', '😂', '😮', '😢'];

  return (
    <div>
      {messages.map(message => {
        if (message.isDeleted) return null;

        const isOwnMessage = message.userId === currentUser.id;
        const isEditing = editingMessageId === message.id;
        const timeAgo = formatDistanceToNow(new Date(message.createdAt), {
          addSuffix: true,
        });
        const isEdited =
          new Date(message.updatedAt) > new Date(message.createdAt);

        return (
          <div
            key={message.id}
            style={{
              marginBottom: '1rem',
              padding: '0.75rem',
              background: isOwnMessage
                ? 'var(--bg-hover)'
                : 'var(--bg-secondary)',
              borderRadius: '8px',
              border: '1px solid var(--border)',
            }}
          >
            {/* Message header */}
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                marginBottom: '0.5rem',
              }}
            >
              <div
                style={{
                  fontWeight: 'bold',
                  color: isOwnMessage ? 'var(--accent)' : 'var(--text-primary)',
                }}
              >
                {message.displayName}
              </div>
              <div
                style={{
                  fontSize: '0.8rem',
                  color: 'var(--text-secondary)',
                }}
              >
                {timeAgo}
                {isEdited && ' (edited)'}
                {message.expiresAt && (
                  <span
                    style={{ color: 'var(--warning)', marginLeft: '0.5rem' }}
                  >
                    expires {formatDistanceToNow(new Date(message.expiresAt))}
                  </span>
                )}
              </div>
            </div>

            {/* Message content */}
            {isEditing ? (
              <div style={{ marginBottom: '0.5rem' }}>
                <textarea
                  value={editContent}
                  onChange={e => setEditContent(e.target.value)}
                  style={{
                    width: '100%',
                    padding: '0.5rem',
                    background: 'var(--bg-tertiary)',
                    border: '1px solid var(--border)',
                    borderRadius: '4px',
                    color: 'var(--text-primary)',
                    resize: 'vertical',
                    minHeight: '60px',
                  }}
                  maxLength={2000}
                />
                <div style={{ marginTop: '0.5rem' }}>
                  <button
                    onClick={handleEditSave}
                    style={{
                      padding: '0.25rem 0.5rem',
                      marginRight: '0.5rem',
                      background: 'var(--success)',
                      border: 'none',
                      borderRadius: '4px',
                      color: 'white',
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                    }}
                  >
                    Save
                  </button>
                  <button
                    onClick={handleEditCancel}
                    style={{
                      padding: '0.25rem 0.5rem',
                      background: 'var(--error)',
                      border: 'none',
                      borderRadius: '4px',
                      color: 'white',
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                    }}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            ) : (
              <div
                style={{
                  marginBottom: '0.5rem',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                }}
              >
                {message.content}
              </div>
            )}

            {/* Reactions */}
            {message.reactions && message.reactions.length > 0 && (
              <div
                style={{
                  display: 'flex',
                  flexWrap: 'wrap',
                  gap: '0.25rem',
                  marginBottom: '0.5rem',
                }}
              >
                {message.reactions.map(reaction => (
                  <button
                    key={reaction.emoji}
                    onClick={() =>
                      handleReactionClick(
                        message.id,
                        reaction.emoji,
                        reaction.users.includes(currentUser.displayName)
                      )
                    }
                    style={{
                      padding: '0.25rem 0.5rem',
                      background: reaction.users.includes(
                        currentUser.displayName
                      )
                        ? 'var(--accent)'
                        : 'var(--bg-tertiary)',
                      border: '1px solid var(--border)',
                      borderRadius: '12px',
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '0.25rem',
                    }}
                    title={`Reacted by: ${reaction.users.join(', ')}`}
                  >
                    <span>{reaction.emoji}</span>
                    <span>{reaction.count}</span>
                  </button>
                ))}
              </div>
            )}

            {/* Message actions */}
            {!isEditing && (
              <div
                style={{
                  display: 'flex',
                  gap: '0.5rem',
                  alignItems: 'center',
                }}
              >
                {/* Add reaction buttons */}
                {availableEmojis.map(emoji => (
                  <button
                    key={emoji}
                    onClick={() => onAddReaction(message.id, emoji)}
                    style={{
                      padding: '0.25rem',
                      background: 'transparent',
                      border: '1px solid var(--border-light)',
                      borderRadius: '4px',
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                      color: 'var(--text-secondary)',
                    }}
                    title={`Add ${emoji} reaction`}
                  >
                    {emoji}
                  </button>
                ))}

                {/* Edit button (only for own messages) */}
                {isOwnMessage && (
                  <button
                    onClick={() => handleEditStart(message)}
                    style={{
                      padding: '0.25rem 0.5rem',
                      background: 'transparent',
                      border: '1px solid var(--border-light)',
                      borderRadius: '4px',
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                      color: 'var(--text-secondary)',
                    }}
                  >
                    Edit
                  </button>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

export default MessageList;
