import { useState } from 'react';
import { DbConnection, User } from '../module_bindings';

interface StatusDropdownProps {
  conn: DbConnection;
  currentUser: User;
}

const STATUS_OPTIONS = [
  { value: 'online', label: 'Online', color: 'var(--online)' },
  { value: 'away', label: 'Away', color: 'var(--away)' },
  { value: 'do_not_disturb', label: 'Do Not Disturb', color: 'var(--dnd)' },
  { value: 'invisible', label: 'Invisible', color: 'var(--offline)' },
];

export default function StatusDropdown({
  conn,
  currentUser,
}: StatusDropdownProps) {
  const [isOpen, setIsOpen] = useState(false);

  const handleStatusChange = (status: string) => {
    conn.reducers.setStatus({ status });
    setIsOpen(false);
  };

  const currentStatus =
    STATUS_OPTIONS.find(s => s.value === currentUser.status) ??
    STATUS_OPTIONS[0];

  return (
    <div className="status-dropdown">
      <div
        className="user-avatar"
        onClick={() => setIsOpen(!isOpen)}
        style={{ cursor: 'pointer' }}
      >
        {(currentUser.name ?? '?')[0].toUpperCase()}
        <div className={`status-dot ${currentUser.status}`}></div>
      </div>
      <div className="user-info">
        <div className="user-name">{currentUser.name}</div>
        <div className="user-status">{currentStatus.label}</div>
      </div>

      {isOpen && (
        <div className="status-dropdown-menu">
          {STATUS_OPTIONS.map(option => (
            <button
              key={option.value}
              className="status-option"
              onClick={() => handleStatusChange(option.value)}
            >
              <div
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: '50%',
                  background: option.color,
                }}
              />
              {option.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
