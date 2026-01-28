import React, { useState } from 'react';

interface CreateRoomModalProps {
  onCreateRoom: (name: string) => void;
  onClose: () => void;
}

function CreateRoomModal({ onCreateRoom, onClose }: CreateRoomModalProps) {
  const [roomName, setRoomName] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (roomName.trim()) {
      onCreateRoom(roomName.trim());
      setRoomName('');
      onClose();
    }
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  return (
    <div
      style={{
        position: 'fixed',
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        background: 'rgba(0, 0, 0, 0.5)',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        zIndex: 1000,
      }}
      onClick={handleBackdropClick}
    >
      <div
        style={{
          background: 'var(--bg-secondary)',
          padding: '2rem',
          borderRadius: '8px',
          border: '1px solid var(--border)',
          width: '100%',
          maxWidth: '400px',
        }}
      >
        <h3 style={{
          color: 'var(--accent)',
          marginBottom: '1.5rem',
          textAlign: 'center',
        }}>
          Create New Room
        </h3>

        <form onSubmit={handleSubmit}>
          <div style={{ marginBottom: '1.5rem' }}>
            <label
              htmlFor="roomName"
              style={{
                display: 'block',
                marginBottom: '0.5rem',
                color: 'var(--text-secondary)',
                fontSize: '0.9rem',
              }}
            >
              Room Name
            </label>
            <input
              id="roomName"
              type="text"
              value={roomName}
              onChange={(e) => setRoomName(e.target.value)}
              placeholder="Enter room name"
              style={{
                width: '100%',
                padding: '0.75rem',
                background: 'var(--bg-tertiary)',
                border: '1px solid var(--border)',
                borderRadius: '4px',
                color: 'var(--text-primary)',
                fontSize: '1rem',
              }}
              maxLength={100}
              required
            />
          </div>

          <div style={{
            display: 'flex',
            gap: '1rem',
            justifyContent: 'flex-end',
          }}>
            <button
              type="button"
              onClick={onClose}
              style={{
                padding: '0.75rem 1.5rem',
                background: 'transparent',
                border: '1px solid var(--border-light)',
                borderRadius: '4px',
                color: 'var(--text-secondary)',
                cursor: 'pointer',
                fontSize: '1rem',
              }}
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!roomName.trim()}
              style={{
                padding: '0.75rem 1.5rem',
                background: roomName.trim() ? 'var(--accent)' : 'var(--border-light)',
                border: 'none',
                borderRadius: '4px',
                color: 'var(--bg-primary)',
                cursor: roomName.trim() ? 'pointer' : 'not-allowed',
                fontSize: '1rem',
                fontWeight: 'bold',
              }}
            >
              Create Room
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export default CreateRoomModal;