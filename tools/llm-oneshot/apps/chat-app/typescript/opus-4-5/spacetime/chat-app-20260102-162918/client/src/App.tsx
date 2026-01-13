import { useState, useEffect, useRef, useCallback } from 'react';
import { useTable, Identity } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import UserSetup from './components/UserSetup';
import InvitesPanel from './components/InvitesPanel';
import './styles.css';

export default function App() {
  const [conn, setConn] = useState<DbConnection | null>(window.__db_conn);
  const [myIdentity, setMyIdentity] = useState<Identity | null>(window.__my_identity);
  const [selectedRoomId, setSelectedRoomId] = useState<bigint | null>(null);
  const [showInvites, setShowInvites] = useState(false);

  const [users, usersLoading] = useTable(tables.user);
  const [rooms, roomsLoading] = useTable(tables.room);
  const [roomMembers, membersLoading] = useTable(tables.roomMember);
  const [roomInvites] = useTable(tables.roomInvite);

  // Poll for connection availability
  useEffect(() => {
    const interval = setInterval(() => {
      if (window.__db_conn && !conn) setConn(window.__db_conn);
      if (window.__my_identity && !myIdentity) setMyIdentity(window.__my_identity);
    }, 100);
    return () => clearInterval(interval);
  }, [conn, myIdentity]);

  // Heartbeat to keep presence updated
  useEffect(() => {
    if (!conn) return;
    const interval = setInterval(() => {
      conn.reducers.heartbeat({});
    }, 60000);
    return () => clearInterval(interval);
  }, [conn]);

  const isLoading = usersLoading || roomsLoading || membersLoading;

  const currentUser = users?.find(
    u => myIdentity && u.identity.toHexString() === myIdentity.toHexString()
  );

  const myRoomIds = new Set(
    roomMembers
      ?.filter(m => myIdentity && m.userId.toHexString() === myIdentity.toHexString())
      .map(m => m.roomId) ?? []
  );

  // Filter rooms: show public rooms OR rooms I'm a member of
  const visibleRooms = rooms?.filter(r => !r.isPrivate || myRoomIds.has(r.id)) ?? [];

  const pendingInvites = roomInvites?.filter(
    inv => myIdentity && inv.inviteeId.toHexString() === myIdentity.toHexString() && inv.status === 'pending'
  ) ?? [];

  if (!conn || isLoading) {
    return (
      <div className="loading-screen">
        <div className="loading-spinner"></div>
        <p>Connecting to SpacetimeDB...</p>
      </div>
    );
  }

  if (!currentUser?.name) {
    return <UserSetup conn={conn} />;
  }

  return (
    <div className="app-container">
      <Sidebar
        conn={conn}
        rooms={visibleRooms}
        myRoomIds={myRoomIds}
        selectedRoomId={selectedRoomId}
        onSelectRoom={setSelectedRoomId}
        currentUser={currentUser}
        users={users ?? []}
        pendingInvitesCount={pendingInvites.length}
        onShowInvites={() => setShowInvites(true)}
        myIdentity={myIdentity}
      />
      {showInvites ? (
        <InvitesPanel
          conn={conn}
          invites={pendingInvites}
          rooms={rooms ?? []}
          users={users ?? []}
          onClose={() => setShowInvites(false)}
        />
      ) : selectedRoomId != null ? (
        <ChatArea
          conn={conn}
          roomId={selectedRoomId}
          myIdentity={myIdentity}
          users={users ?? []}
          roomMembers={roomMembers ?? []}
        />
      ) : (
        <div className="no-room-selected">
          <h2>Welcome to Chat App</h2>
          <p>Select a room from the sidebar or create a new one to get started.</p>
        </div>
      )}
    </div>
  );
}
