import React, { useEffect, useState } from 'react';
import { NavLink, useNavigate } from 'react-router-dom';
import { Plus, Hash } from 'lucide-react';
import { Room } from '../types';
import { socket } from '../socket';

export default function RoomList() {
  const [rooms, setRooms] = useState<Room[]>([]);
  const navigate = useNavigate();

  const fetchRooms = () => {
    const token = localStorage.getItem('token');
    fetch('/api/rooms', { headers: { Authorization: `Bearer ${token}` } })
      .then(res => res.json())
      .then(data => setRooms(data))
      .catch(console.error);
  };

  useEffect(() => {
    fetchRooms();

    socket.on('room:created', (newRoom) => {
      setRooms(prev => [newRoom, ...prev]);
    });

    // Poll for unread counts (simple fallback for real-time without complex socket arch)
    const interval = setInterval(fetchRooms, 5000);

    return () => {
      socket.off('room:created');
      clearInterval(interval);
    };
  }, []);

  const createRoom = async () => {
    const name = prompt('Enter room name:');
    if (!name) return;
    
    const token = localStorage.getItem('token');
    try {
      const res = await fetch('/api/rooms', {
        method: 'POST',
        headers: { 
          'Content-Type': 'application/json',
          Authorization: `Bearer ${token}`
        },
        body: JSON.stringify({ name })
      });
      if (res.ok) {
        const room = await res.json();
        navigate(`/rooms/${room.id}`);
      } else {
        alert('Failed to create room');
      }
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div style={{ width: 240, background: 'var(--bg-secondary)', display: 'flex', flexDirection: 'column', height: '100vh' }}>
      <div style={{ padding: 16, borderBottom: '1px solid var(--bg-tertiary)', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <h2 style={{ margin: 0, fontSize: 16 }}>Rooms</h2>
        <button onClick={createRoom} className="btn" style={{ padding: 4, background: 'transparent', color: 'var(--text-normal)' }}>
          <Plus size={20} />
        </button>
      </div>
      <div style={{ flex: 1, overflowY: 'auto', padding: 8 }}>
        {rooms.map(room => (
          <NavLink
            key={room.id}
            to={`/rooms/${room.id}`}
            style={({ isActive }) => ({
              display: 'flex',
              alignItems: 'center',
              padding: '8px 12px',
              textDecoration: 'none',
              color: isActive ? 'white' : 'var(--text-muted)',
              backgroundColor: isActive ? 'var(--message-hover)' : 'transparent',
              borderRadius: 4,
              marginBottom: 2,
              justifyContent: 'space-between'
            })}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, overflow: 'hidden' }}>
              <Hash size={18} />
              <span style={{ textOverflow: 'ellipsis', overflow: 'hidden', whiteSpace: 'nowrap' }}>{room.name}</span>
            </div>
            {room.unreadCount ? (
              <span style={{ 
                background: 'var(--danger)', 
                color: 'white', 
                fontSize: 10, 
                borderRadius: 10, 
                padding: '2px 6px', 
                fontWeight: 'bold' 
              }}>
                {room.unreadCount}
              </span>
            ) : null}
          </NavLink>
        ))}
      </div>
      <div style={{ padding: 16, borderTop: '1px solid var(--bg-tertiary)' }}>
        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
          Logged in as <strong>{JSON.parse(atob(localStorage.getItem('token')?.split('.')[1] || '{}')).username}</strong>
        </div>
      </div>
    </div>
  );
}
