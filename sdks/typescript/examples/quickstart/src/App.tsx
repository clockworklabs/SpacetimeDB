import './App.css';

import { DbConnection } from './module_bindings';

import { Identity } from '@clockworklabs/spacetimedb-sdk';
import React, { useEffect, useRef, useState } from 'react';

export type MessageType = {
  name: string;
  message: string;
};

const token = localStorage.getItem('auth_token') || undefined;
const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('chat')
  .build();

function App() {
  const [newName, setNewName] = useState('');
  const [settingName, setSettingName] = useState(false);
  const [name, setName] = useState('');

  // Store all system messages as a Set, to avoid duplication
  const [systemMessages, setSystemMessages] = useState(
    () => new Set<string>([])
  );
  const [messages, setMessages] = useState<MessageType[]>([]);

  const [newMessage, setNewMessage] = useState('');

  const local_identity = useRef<Identity | undefined>(undefined);
  const initialized = useRef<boolean>(false);

  useEffect(() => {
    if (!initialized.current) {
      conn.connect();
      initialized.current = true;
    }
  }, []);

  // All the event listeners are set up in the useEffect hook
  useEffect(() => {
    conn.on('disconnected', () => {
      console.log('disconnected');
    });

    conn.on('client_error', () => {
      console.log('client_error');
    });

    conn.onConnect((token: string, identity: Identity) => {
      console.log('Connected to SpacetimeDB');

      local_identity.current = identity;

      localStorage.setItem('auth_token', token);

      conn.subscribe(['SELECT * FROM User', 'SELECT * FROM Message']);
    });

    conn.on('initialStateSync', () => {
      setAllMessagesInOrder();
      const user = User.findByIdentity(local_identity?.current!);
      setName(userNameOrIdentity(user!));
    });

    User.onInsert(user => {
      if (user.online) {
        appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);
      }
    });

    User.onUpdate((oldUser, user) => {
      if (oldUser.online === false && user.online === true) {
        appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);
      } else if (oldUser.online === true && user.online === false) {
        appendToSystemMessage(`${userNameOrIdentity(user)} has disconnected.`);
      }

      if (user.name !== oldUser.name) {
        appendToSystemMessage(
          `User ${userNameOrIdentity(oldUser)} renamed to ${userNameOrIdentity(
            user
          )}.`
        );
      }
    });

    Message.onInsert(() => {
      setAllMessagesInOrder();
    });

    SendMessageReducer.on(reducerEvent => {
      if (
        local_identity.current &&
        reducerEvent.callerIdentity.isEqual(local_identity.current)
      ) {
        if (reducerEvent.status === 'failed') {
          appendToSystemMessage(
            `Error sending message: ${reducerEvent.message} `
          );
        }
      }
    });

    SetNameReducer.on((reducerEvent, reducerArgs) => {
      if (
        local_identity.current &&
        reducerEvent.callerIdentity.isEqual(local_identity.current)
      ) {
        if (reducerEvent.status === 'failed') {
          appendToSystemMessage(`Error setting name: ${reducerEvent.message} `);
        } else if (reducerEvent.status === 'committed') {
          setName(reducerArgs[0]);
        }
      }
    });
  }, []);

  function userNameOrIdentity(user: User): string {
    console.log(`Name: ${user.name} `);
    if (user.name !== null) return user.name || '';

    const identityStr = user.identity.toHexString();
    console.log(`Name: ${identityStr} `);
    return user.identity.toHexString().substring(0, 8);
  }

  function setAllMessagesInOrder() {
    const messages = Array.from(Message.all());
    messages.sort((a, b) => (a.sent > b.sent ? 1 : a.sent < b.sent ? -1 : 0));

    const messagesType: MessageType[] = messages.map(message => {
      const sender = User.findByIdentity(message.sender);
      const name = sender ? userNameOrIdentity(sender) : 'unknown';

      return {
        name: name, // convert sender Uint8Array to name string using helper function
        message: message.text, // map text to message
      };
    });

    setMessages(messagesType);
  }

  // Helper function to append a line to the systemMessage state
  function appendToSystemMessage(line: string) {
    setSystemMessages(systemMessages => systemMessages.add(line));
  }

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    SetNameReducer.call(newName);
    setSettingName(false);
  };

  const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // send message here
    SendMessageReducer.call(newMessage);
    setNewMessage('');
  };

  return (
    <div className="App">
      <div className="profile">
        <h2>Profile</h2>
        {!settingName ? (
          <>
            <p>{name}</p>
            <button
              type="button"
              onClick={() => {
                setSettingName(true);
                setNewName(name);
              }}
            >
              EDIT NAME
            </button>
          </>
        ) : (
          <form onSubmit={onSubmitNewName}>
            <input
              type="text"
              style={{ marginBottom: '1rem' }}
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />
            <button type="submit">SUBMIT</button>
          </form>
        )}
      </div>

      <section className="chatbox">
        <div className="message">
          <h2>Messages</h2>
          {messages.length < 1 && <p>No messages</p>}
          <div>
            {messages.map(({ message, name }) => (
              <div key={message}>
                <p>
                  <b>{name}</b>: {message}
                </p>
              </div>
            ))}
          </div>
        </div>

        <div className="new-message">
          <form onSubmit={onMessageSubmit}>
            <input
              value={newMessage}
              onChange={e => setNewMessage(e.target.value)}
              placeholder="Send a message..."
              autoFocus
              type="text"
            />
            <button type="submit">Send</button>
          </form>
        </div>
      </section>

      <div className="system" style={{ whiteSpace: 'pre-wrap' }}>
        <h2>System</h2>
        <div>
          {Array.from(systemMessages).map(message => (
            <p key={message}>{message}</p>
          ))}
        </div>
      </div>
    </div>
  );
}

export default App;
