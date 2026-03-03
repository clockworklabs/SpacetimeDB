import { DbConnection, ScheduledMessage } from '../module_bindings';

interface ScheduledMessagesPanelProps {
  conn: DbConnection;
  scheduledMessages: ScheduledMessage[];
  onClose: () => void;
}

export default function ScheduledMessagesPanel({
  conn,
  scheduledMessages,
  onClose,
}: ScheduledMessagesPanelProps) {
  const handleCancel = (scheduledId: bigint) => {
    if (confirm('Cancel this scheduled message?')) {
      conn.reducers.cancelScheduledMessage({ scheduledId });
    }
  };

  const formatScheduledTime = (scheduleAt: { tag: string; value: bigint }) => {
    // ScheduleAt stores time as microseconds
    const date = new Date(Number(scheduleAt.value / 1000n));
    return date.toLocaleString();
  };

  return (
    <div
      style={{
        padding: '16px',
        background: 'var(--bg-secondary)',
        borderBottom: '1px solid var(--border)',
      }}
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '12px',
        }}
      >
        <h4 style={{ margin: 0 }}>Scheduled Messages</h4>
        <button className="btn-icon" onClick={onClose}>
          ✕
        </button>
      </div>

      {scheduledMessages.map(sm => (
        <div key={sm.scheduledId.toString()} className="scheduled-message">
          <div className="scheduled-message-header">
            <span className="scheduled-time">
              📅{' '}
              {formatScheduledTime(
                sm.scheduledAt as unknown as { tag: string; value: bigint }
              )}
            </span>
            <button
              className="btn btn-danger btn-small"
              onClick={() => handleCancel(sm.scheduledId)}
            >
              Cancel
            </button>
          </div>
          <div style={{ color: 'var(--text-primary)' }}>{sm.content}</div>
        </div>
      ))}
    </div>
  );
}
