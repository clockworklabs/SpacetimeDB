import React, { useState } from 'react';
import './App.css';
import { DbConnection, Message, User } from './module_bindings';
import { useSpacetimeDB, useTable } from '@clockworklabs/spacetimedb-sdk/react';

export type PrettyMessage = {
  senderName: string;
  text: string;
};

function App() {
  const conn = useSpacetimeDB<DbConnection>();
  const { identity, isActive: connected } = conn;
  const [newName, setNewName] = useState('');
  const [settingName, setSettingName] = useState(false);
  const [systemMessage, setSystemMessage] = useState('');
  const [newMessage, setNewMessage] = useState('');

  const { rows: messages } = useTable<DbConnection, Message>('message');
  const { rows: users } = useTable<DbConnection, User>('user', {
    onInsert: user => {
      if (user.online) {
        const name = user.name || user.identity.toHexString().substring(0, 8);
        setSystemMessage(prev => prev + `\n${name} has connected.`);
      }
    },
    onUpdate: (oldUser, newUser) => {
      const name =
        newUser.name || newUser.identity.toHexString().substring(0, 8);
      if (oldUser.online === false && newUser.online === true) {
        setSystemMessage(prev => prev + `\n${name} has connected.`);
      } else if (oldUser.online === true && newUser.online === false) {
        setSystemMessage(prev => prev + `\n${name} has disconnected.`);
      }
    },
    onDelete: user => {
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessage(prev => prev + `\n${name} has disconnected.`);
    },
  });

  const prettyMessages: PrettyMessage[] = Array.from(messages)
    .sort((a, b) => (a.sent > b.sent ? 1 : -1))
    .map(message => {
      const user = users.find(
        u => u.identity.toHexString() === message.sender.toHexString()
      );
      return {
        senderName: user?.name || message.sender.toHexString().substring(0, 8),
        text: message.text,
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
    conn.reducers.setName(newName);
  };

  const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setNewMessage('');
    conn.reducers.sendMessage(newMessage);
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
              aria-label="name input"
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />
            <button type="submit">Submit</button>
          </form>
        )}
      </div>
      <div className="message">
        <h1>Messages</h1>
        {prettyMessages.length < 1 && <p>No messages</p>}
        <div>
          {prettyMessages.map((message, key) => (
            <div key={key}>
              <p>
                <b>{message.senderName}</b>
              </p>
              <p>{message.text}</p>
            </div>
          ))}
        </div>
      </div>
      <div className="system" style={{ whiteSpace: 'pre-wrap' }}>
        <h1>System</h1>
        <div>
          <p>{systemMessage}</p>
        </div>
      </div>
      <div className="new-message">
        <form
          onSubmit={onMessageSubmit}
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
