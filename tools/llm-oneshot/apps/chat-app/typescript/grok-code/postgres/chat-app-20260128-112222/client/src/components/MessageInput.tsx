import React, { useState, useRef, useEffect } from 'react';

interface MessageInputProps {
  roomId: string;
  onSendMessage: (roomId: string, content: string, scheduledFor?: Date, expiresAt?: Date) => void;
  onStartTyping: (roomId: string) => void;
  onStopTyping: (roomId: string) => void;
}

function MessageInput({ roomId, onSendMessage, onStartTyping, onStopTyping }: MessageInputProps) {
  const [content, setContent] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [scheduledFor, setScheduledFor] = useState('');
  const [expiresIn, setExpiresIn] = useState('');
  const typingTimeoutRef = useRef<NodeJS.Timeout>();
  const isTypingRef = useRef(false);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (content.trim()) {
      let scheduledDate: Date | undefined;
      let expiresDate: Date | undefined;

      if (scheduledFor) {
        scheduledDate = new Date(scheduledFor);
        if (isNaN(scheduledDate.getTime())) {
          alert('Invalid scheduled time');
          return;
        }
      }

      if (expiresIn) {
        const minutes = parseInt(expiresIn);
        if (minutes > 0) {
          expiresDate = new Date(Date.now() + minutes * 60 * 1000);
        }
      }

      onSendMessage(roomId, content.trim(), scheduledDate, expiresDate);
      setContent('');
      setScheduledFor('');
      setExpiresIn('');
      setShowAdvanced(false);
      stopTyping();
    }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setContent(e.target.value);

    // Handle typing indicators
    if (e.target.value && !isTypingRef.current) {
      isTypingRef.current = true;
      onStartTyping(roomId);
    }

    // Clear existing timeout
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    // Set new timeout to stop typing
    typingTimeoutRef.current = setTimeout(() => {
      stopTyping();
    }, 3000);
  };

  const stopTyping = () => {
    if (isTypingRef.current) {
      isTypingRef.current = false;
      onStopTyping(roomId);
    }
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }
  };

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (typingTimeoutRef.current) {
        clearTimeout(typingTimeoutRef.current);
      }
      stopTyping();
    };
  }, []);

  return (
    <div style={{
      padding: '1rem',
      borderTop: '1px solid var(--border)',
      background: 'var(--bg-secondary)',
    }}>
      <form onSubmit={handleSubmit}>
        <div style={{ marginBottom: '0.5rem' }}>
          <textarea
            value={content}
            onChange={handleInputChange}
            placeholder="Type your message..."
            style={{
              width: '100%',
              minHeight: '60px',
              padding: '0.75rem',
              background: 'var(--bg-tertiary)',
              border: '1px solid var(--border)',
              borderRadius: '4px',
              color: 'var(--text-primary)',
              resize: 'vertical',
              fontFamily: 'inherit',
              fontSize: '1rem',
            }}
            maxLength={2000}
            required
          />
        </div>

        {/* Advanced options toggle */}
        <div style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '0.5rem',
        }}>
          <button
            type="button"
            onClick={() => setShowAdvanced(!showAdvanced)}
            style={{
              padding: '0.25rem 0.5rem',
              background: 'transparent',
              border: '1px solid var(--border-light)',
              borderRadius: '4px',
              color: 'var(--text-secondary)',
              cursor: 'pointer',
              fontSize: '0.8rem',
            }}
          >
            {showAdvanced ? 'Hide' : 'Show'} Advanced Options
          </button>

          <button
            type="submit"
            disabled={!content.trim()}
            style={{
              padding: '0.75rem 1.5rem',
              background: content.trim() ? 'var(--accent)' : 'var(--border-light)',
              border: 'none',
              borderRadius: '4px',
              color: 'var(--bg-primary)',
              cursor: content.trim() ? 'pointer' : 'not-allowed',
              fontSize: '1rem',
              fontWeight: 'bold',
            }}
          >
            Send
          </button>
        </div>

        {/* Advanced options */}
        {showAdvanced && (
          <div style={{
            background: 'var(--bg-tertiary)',
            padding: '1rem',
            borderRadius: '4px',
            border: '1px solid var(--border)',
            marginBottom: '0.5rem',
          }}>
            <div style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '1rem',
            }}>
              <div>
                <label
                  htmlFor="scheduledFor"
                  style={{
                    display: 'block',
                    marginBottom: '0.25rem',
                    color: 'var(--text-secondary)',
                    fontSize: '0.9rem',
                  }}
                >
                  Schedule for (optional)
                </label>
                <input
                  id="scheduledFor"
                  type="datetime-local"
                  value={scheduledFor}
                  onChange={(e) => setScheduledFor(e.target.value)}
                  style={{
                    width: '100%',
                    padding: '0.5rem',
                    background: 'var(--bg-primary)',
                    border: '1px solid var(--border)',
                    borderRadius: '4px',
                    color: 'var(--text-primary)',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="expiresIn"
                  style={{
                    display: 'block',
                    marginBottom: '0.25rem',
                    color: 'var(--text-secondary)',
                    fontSize: '0.9rem',
                  }}
                >
                  Expires in (minutes, optional)
                </label>
                <input
                  id="expiresIn"
                  type="number"
                  value={expiresIn}
                  onChange={(e) => setExpiresIn(e.target.value)}
                  placeholder="e.g., 5"
                  min="1"
                  max="1440"
                  style={{
                    width: '100%',
                    padding: '0.5rem',
                    background: 'var(--bg-primary)',
                    border: '1px solid var(--border)',
                    borderRadius: '4px',
                    color: 'var(--text-primary)',
                  }}
                />
              </div>
            </div>

            <div style={{
              marginTop: '0.5rem',
              fontSize: '0.8rem',
              color: 'var(--text-muted)',
            }}>
              • Scheduled messages will be sent at the specified time<br/>
              • Ephemeral messages will auto-delete after the specified duration
            </div>
          </div>
        )}
      </form>
    </div>
  );
}

export default MessageInput;