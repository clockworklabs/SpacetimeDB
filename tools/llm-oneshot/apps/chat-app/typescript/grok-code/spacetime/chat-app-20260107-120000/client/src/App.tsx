import { useState, useEffect } from 'react';
import { useTable } from 'spacetimedb/react';
import { tables } from './module_bindings';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import UserSetup from './components/UserSetup';
import './index.css';

function App() {
  const [currentRoomId, setCurrentRoomId] = useState<bigint | null>(null);
  const [myIdentity, setMyIdentity] = useState<string | null>(null);

  // Get user data
  const [users] = useTable(tables.user);
  const [userStatuses] = useTable(tables.userStatus);

  // Get current user
  const currentUser = users.find(u => u.identity.toHexString() === myIdentity);

  useEffect(() => {
    // Get identity from global state
    const checkIdentity = () => {
      if (window.__my_identity) {
        setMyIdentity(window.__my_identity);
      } else {
        // Check again in a moment
        setTimeout(checkIdentity, 100);
      }
    };
    checkIdentity();
  }, []);

  // Show user setup if no user data
  if (!currentUser) {
    return <UserSetup onUserCreated={() => {}} />;
  }

  return (
    <div className="app">
      <Sidebar
        currentRoomId={currentRoomId}
        onRoomSelect={setCurrentRoomId}
        currentUser={currentUser}
        users={users}
        userStatuses={userStatuses}
      />
      <ChatArea
        roomId={currentRoomId}
        currentUser={currentUser}
        users={users}
      />
    </div>
  );
}

export default App;