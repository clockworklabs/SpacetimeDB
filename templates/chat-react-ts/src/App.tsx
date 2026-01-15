import React, { useState } from 'react';
import './App.css';
import { tables, reducers, Message } from './module_bindings';
import {
  useSpacetimeDB,
  useTable,
  where,
  eq,
  useReducer,
} from 'spacetimedb/react';
import { Identity, Infer, Timestamp } from 'spacetimedb';

export type PrettyMessage = {
  senderName: string;
  text: string;
  sent: Timestamp;
  kind: 'system' | 'user';
};

function App() {
  const [newName, setNewName] = useState('');
  const [settingName, setSettingName] = useState(false);
  const [systemMessages, setSystemMessages] = useState(
    [] as Infer<typeof Message>[]
  );
  const [newMessage, setNewMessage] = useState('');

  const { identity, isActive: connected } = useSpacetimeDB();
  const setName = useReducer(reducers.setName);
  const sendMessage = useReducer(reducers.sendMessage);

  // Subscribe to all messages in the chat
  const [messages] = useTable(tables.message);

  // Subscribe to all online users in the chat
  // so we can show who's online and demonstrate
  // the `where` and `eq` query expressions
  const [onlineUsers] = useTable(tables.user, where(eq('online', true)), {
    onInsert: user => {
      // All users being inserted here are online
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has connected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
    onDelete: user => {
      // All users being deleted here are offline
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has disconnected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
  });

  const [offlineUsers] = useTable(tables.user, where(eq('online', false)));
  const users = [...onlineUsers, ...offlineUsers];

  const prettyMessages: PrettyMessage[] = messages
    .concat(systemMessages)
    .sort((a, b) => (a.sent.toDate() > b.sent.toDate() ? 1 : -1))
    .map(message => {
      const user = users.find(
        u => u.identity.toHexString() === message.sender.toHexString()
      );
      return {
        senderName: user?.name || message.sender.toHexString().substring(0, 8),
        text: message.text,
        sent: message.sent,
        kind: Identity.zero().isEqual(message.sender) ? 'system' : 'user',
      };
    });

  console.log('connected:', connected, 'identity:', identity?.toHexString());

  if (!connected || !identity) {
    return (
      <div className="App">
        <h1>Connecting...</h1>
      </div>
    );
  }

  const name = (() => {
    const user = users.find(u => u.identity.isEqual(identity));
    return user?.name || identity?.toHexString().substring(0, 8) || '';
  })();

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setSettingName(false);
    setName({ name: newName });
  };

  const onSubmitMessage = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setNewMessage('');
    sendMessage({ text: newMessage });
  };

  return (
    <div className="App">
      <div className="profile">
        <h1>Profile</h1>
        {!settingName ? (
          <>
            <p>{name}</p>
            <button
              onClick={() => {
                setSettingName(true);
                setNewName(name);
              }}
            >
              Edit Name
            </button>
          </>
        ) : (
          <form onSubmit={onSubmitNewName}>
            <input
              type="text"
              aria-label="username input"
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />
            <button type="submit">Submit</button>
          </form>
        )}
      </div>
      <div className="message-panel">
        <h1>Messages</h1>
        {prettyMessages.length < 1 && <p>No messages</p>}
        <div className="messages">
          {prettyMessages.map((message, key) => {
            const sentDate = message.sent.toDate();
            const now = new Date();
            const isOlderThanDay =
              now.getFullYear() !== sentDate.getFullYear() ||
              now.getMonth() !== sentDate.getMonth() ||
              now.getDate() !== sentDate.getDate();

            const timeString = sentDate.toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
            });
            const dateString = isOlderThanDay
              ? sentDate.toLocaleDateString([], {
                  year: 'numeric',
                  month: 'short',
                  day: 'numeric',
                }) + ' '
              : '';

            return (
              <div
                key={key}
                className={
                  message.kind === 'system' ? 'system-message' : 'user-message'
                }
              >
                <p>
                  <b>
                    {message.kind === 'system' ? 'System' : message.senderName}
                  </b>
                  <span
                    style={{
                      fontSize: '0.8rem',
                      marginLeft: '0.5rem',
                      color: '#666',
                    }}
                  >
                    {dateString}
                    {timeString}
                  </span>
                </p>
                <p>{message.text}</p>
              </div>
            );
          })}
        </div>
      </div>
      <div className="online" style={{ whiteSpace: 'pre-wrap' }}>
        <h1>Online</h1>
        <div>
          {onlineUsers.map((user, key) => (
            <div key={key}>
              <p>{user.name || user.identity.toHexString().substring(0, 8)}</p>
            </div>
          ))}
        </div>
        {offlineUsers.length > 0 && (
          <div>
            <h1>Offline</h1>
            {offlineUsers.map((user, key) => (
              <div key={key}>
                <p>
                  {user.name || user.identity.toHexString().substring(0, 8)}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="new-message">
        <form
          onSubmit={onSubmitMessage}
          style={{
            display: 'flex',
            flexDirection: 'column',
            width: '50%',
            margin: '0 auto',
          }}
        >
          <h3>New Message</h3>
          <textarea
            aria-label="message input"
            value={newMessage}
            onChange={e => setNewMessage(e.target.value)}
          ></textarea>
          <button type="submit">Send</button>
        </form>
      </div>
    </div>
  );
}

export default App;
