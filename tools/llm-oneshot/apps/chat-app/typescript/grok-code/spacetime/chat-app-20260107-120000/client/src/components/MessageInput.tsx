import React, { useState, useRef, useEffect } from 'react';

interface MessageInputProps {
  roomId: bigint;
  onSendMessage: (content: string) => Promise<void>;
  onStartTyping: () => void;
  onStopTyping: () => void;
}

export default function MessageInput({
  roomId,
  onSendMessage,
  onStartTyping,
  onStopTyping
}: MessageInputProps) {
  const [message, setMessage] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [scheduledDelay, setScheduledDelay] = useState(5);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const typingTimeoutRef = useRef<number>();
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!message.trim() || isSending) return;

    setIsSending(true);
    try {
      await onSendMessage(message.trim());
      setMessage('');
      onStopTyping();
    } catch (error) {
      console.error('Failed to send message:', error);
    } finally {
      setIsSending(false);
    }
  };

  const handleScheduledSubmit = async () => {
    if (!message.trim() || !window.__db_conn) return;

    try {
      await window.__db_conn.reducers.scheduleMessage({
        roomId,
        content: message.trim(),
        delayMinutes: BigInt(scheduledDelay)
      });
      setMessage('');
      setShowAdvanced(false);
      onStopTyping();
    } catch (error) {
      console.error('Failed to schedule message:', error);
    }
  };

  const handleEphemeralSubmit = async () => {
    if (!message.trim() || !window.__db_conn) return;

    try {
      await window.__db_conn.reducers.sendEphemeralMessage({
        roomId,
        content: message.trim(),
        durationMinutes: BigInt(ephemeralDuration)
      });
      setMessage('');
      setShowAdvanced(false);
      onStopTyping();
    } catch (error) {
      console.error('Failed to send ephemeral message:', error);
    }
  };

  // Handle typing indicators
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    setMessage(value);

    // Clear existing timeout
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    // Start typing if we have content
    if (value.trim()) {
      onStartTyping();

      // Stop typing after 3 seconds of inactivity
      typingTimeoutRef.current = setTimeout(() => {
        onStopTyping();
      }, 3000);
    } else {
      onStopTyping();
    }
  };

  // Handle keyboard shortcuts
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (!showAdvanced) {
        handleSubmit(e as any);
      }
    }
  };

  // Auto-resize textarea
  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
      inputRef.current.style.height = `${inputRef.current.scrollHeight}px`;
    }
  }, [message]);

  return (
    <div className="chat-input">
      <form onSubmit={handleSubmit}>
        <div style={{ display: 'flex', gap: '8px', alignItems: 'flex-end' }}>
          <div style={{ flex: 1 }}>
            <textarea
              ref={inputRef}
              value={message}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              placeholder="Type a message..."
              className="input"
              style={{
                resize: 'none',
                minHeight: '40px',
                maxHeight: '120px',
                overflowY: message.split('\n').length > 3 ? 'auto' : 'hidden'
              }}
              disabled={isSending}
            />
          </div>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={!message.trim() || isSending}
            style={{ height: '40px', padding: '0 16px' }}
          >
            {isSending ? '...' : 'Send'}
          </button>
        </div>

        {/* Advanced Options */}
        <div style={{ marginTop: '8px', display: 'flex', gap: '8px', alignItems: 'center' }}>
          <button
            type="button"
            onClick={() => setShowAdvanced(!showAdvanced)}
            className="btn"
            style={{ fontSize: '12px' }}
          >
            {showAdvanced ? '▼' : '▶'} Advanced
          </button>

          {showAdvanced && (
            <>
              <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                <label style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                  Schedule:
                </label>
                <input
                  type="number"
                  value={scheduledDelay}
                  onChange={(e) => setScheduledDelay(Number(e.target.value))}
                  min="1"
                  max="1440"
                  style={{ width: '60px', fontSize: '12px' }}
                  className="input"
                />
                <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>min</span>
                <button
                  type="button"
                  onClick={handleScheduledSubmit}
                  className="btn"
                  style={{ fontSize: '12px' }}
                  disabled={!message.trim()}
                >
                  Schedule
                </button>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                <label style={{ fontSize: '12px', color: 'var(--text-muted)' }}>
                  Ephemeral:
                </label>
                <input
                  type="number"
                  value={ephemeralDuration}
                  onChange={(e) => setEphemeralDuration(Number(e.target.value))}
                  min="1"
                  max="60"
                  style={{ width: '50px', fontSize: '12px' }}
                  className="input"
                />
                <span style={{ fontSize: '12px', color: 'var(--text-muted)' }}>min</span>
                <button
                  type="button"
                  onClick={handleEphemeralSubmit}
                  className="btn"
                  style={{ fontSize: '12px' }}
                  disabled={!message.trim()}
                >
                  Send
                </button>
              </div>
            </>
          )}
        </div>

        {/* Help text */}
        <div style={{ fontSize: '11px', color: 'var(--text-muted)', marginTop: '4px' }}>
          Press Enter to send, Shift+Enter for new line
          {showAdvanced && (
            <span style={{ marginLeft: '16px' }}>
              • Schedule: Send message at a future time
              • Ephemeral: Message disappears after duration
            </span>
          )}
        </div>
      </form>
    </div>
  );
}