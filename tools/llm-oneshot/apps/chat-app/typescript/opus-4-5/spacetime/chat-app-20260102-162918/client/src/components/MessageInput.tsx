import { useState, useRef, useEffect, useCallback } from 'react';
import { DbConnection } from '../module_bindings';

interface MessageInputProps {
  conn: DbConnection;
  roomId: bigint;
  replyToId?: bigint;
  onSent?: () => void;
}

export default function MessageInput({
  conn,
  roomId,
  replyToId,
  onSent,
}: MessageInputProps) {
  const [content, setContent] = useState('');
  const [showOptions, setShowOptions] = useState(false);
  const [isEphemeral, setIsEphemeral] = useState(false);
  const [ephemeralDuration, setEphemeralDuration] = useState(60);
  const [isScheduled, setIsScheduled] = useState(false);
  const [scheduledTime, setScheduledTime] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const typingTimeoutRef = useRef<number | null>(null);

  // Auto-resize textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height =
        Math.min(textareaRef.current.scrollHeight, 200) + 'px';
    }
  }, [content]);

  const handleTyping = useCallback(() => {
    conn.reducers.startTyping({ roomId });

    // Clear existing timeout
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }

    // Stop typing after 3 seconds of inactivity
    typingTimeoutRef.current = window.setTimeout(() => {
      conn.reducers.stopTyping({ roomId });
    }, 3000);
  }, [conn, roomId]);

  const handleSubmit = () => {
    const trimmed = content.trim();
    if (!trimmed) return;

    // Clear typing indicator
    if (typingTimeoutRef.current) {
      clearTimeout(typingTimeoutRef.current);
    }
    conn.reducers.stopTyping({ roomId });

    if (isScheduled && scheduledTime) {
      const scheduledDate = new Date(scheduledTime);
      const sendAtTimestamp = BigInt(scheduledDate.getTime()) * 1000n;
      conn.reducers.scheduleMessage({
        roomId,
        content: trimmed,
        sendAtTimestamp,
      });
    } else if (isEphemeral) {
      conn.reducers.sendEphemeralMessage({
        roomId,
        content: trimmed,
        durationSecs: BigInt(ephemeralDuration),
      });
    } else if (replyToId != null) {
      conn.reducers.replyToMessage({ messageId: replyToId, content: trimmed });
    } else {
      conn.reducers.sendMessage({ roomId, content: trimmed });
    }

    setContent('');
    setShowOptions(false);
    setIsEphemeral(false);
    setIsScheduled(false);
    setScheduledTime('');
    onSent?.();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  // Get minimum datetime for scheduler (now + 1 minute)
  const getMinDateTime = () => {
    const now = new Date();
    now.setMinutes(now.getMinutes() + 1);
    return now.toISOString().slice(0, 16);
  };

  return (
    <div className="message-input-container">
      <div className="message-input-row">
        <div className="message-input-wrapper">
          <textarea
            ref={textareaRef}
            value={content}
            onChange={e => {
              setContent(e.target.value);
              handleTyping();
            }}
            onKeyDown={handleKeyDown}
            placeholder={replyToId ? 'Reply in thread...' : 'Type a message...'}
            rows={1}
          />
        </div>
        <button
          className="btn-icon"
          onClick={() => setShowOptions(!showOptions)}
          title="Message options"
        >
          ⚙️
        </button>
        <button
          className="btn btn-primary"
          onClick={handleSubmit}
          disabled={!content.trim()}
        >
          Send
        </button>
      </div>

      {showOptions && (
        <div className="message-options">
          <label className="checkbox-label" style={{ marginRight: '16px' }}>
            <input
              type="checkbox"
              checked={isEphemeral}
              onChange={e => {
                setIsEphemeral(e.target.checked);
                if (e.target.checked) setIsScheduled(false);
              }}
            />
            Ephemeral message
          </label>
          {isEphemeral && (
            <select
              className="input"
              style={{ width: 'auto' }}
              value={ephemeralDuration}
              onChange={e => setEphemeralDuration(Number(e.target.value))}
            >
              <option value={60}>1 minute</option>
              <option value={300}>5 minutes</option>
              <option value={600}>10 minutes</option>
              <option value={3600}>1 hour</option>
            </select>
          )}

          <label className="checkbox-label" style={{ marginLeft: '16px' }}>
            <input
              type="checkbox"
              checked={isScheduled}
              onChange={e => {
                setIsScheduled(e.target.checked);
                if (e.target.checked) setIsEphemeral(false);
              }}
            />
            Schedule message
          </label>
          {isScheduled && (
            <input
              type="datetime-local"
              className="input"
              style={{ width: 'auto' }}
              value={scheduledTime}
              onChange={e => setScheduledTime(e.target.value)}
              min={getMinDateTime()}
            />
          )}
        </div>
      )}
    </div>
  );
}
