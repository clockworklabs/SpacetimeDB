import React from 'react';
import { Outlet } from 'react-router-dom';
import RoomList from './RoomList';

export default function Layout() {
  return (
    <div style={{ display: 'flex', height: '100vh', width: '100vw' }}>
      <RoomList />
      <div
        style={{
          flex: 1,
          display: 'flex',
          flexDirection: 'column',
          background: 'var(--bg-primary)',
        }}
      >
        <Outlet />
      </div>
    </div>
  );
}
