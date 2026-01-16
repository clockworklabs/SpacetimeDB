import { MessageEdit } from '../module_bindings';

interface EditHistoryModalProps {
  edits: MessageEdit[];
  originalContent: string;
  onClose: () => void;
}

export default function EditHistoryModal({ edits, originalContent, onClose }: EditHistoryModalProps) {
  const sortedEdits = [...edits].sort(
    (a, b) => Number(b.editedAt.microsSinceUnixEpoch - a.editedAt.microsSinceUnixEpoch)
  );

  const formatDateTime = (timestamp: { microsSinceUnixEpoch: bigint }) => {
    const date = new Date(Number(timestamp.microsSinceUnixEpoch / 1000n));
    return date.toLocaleString();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()}>
        <h3>Edit History</h3>

        <div className="edit-history-list">
          <div className="edit-history-item" style={{ background: 'var(--accent)', opacity: 0.8 }}>
            <div className="edit-history-time">Current version</div>
            <div className="edit-history-content">{originalContent}</div>
          </div>

          {sortedEdits.map(edit => (
            <div key={edit.id.toString()} className="edit-history-item">
              <div className="edit-history-time">{formatDateTime(edit.editedAt)}</div>
              <div className="edit-history-content">{edit.previousContent}</div>
            </div>
          ))}
        </div>

        <div className="modal-actions">
          <button className="btn btn-secondary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
