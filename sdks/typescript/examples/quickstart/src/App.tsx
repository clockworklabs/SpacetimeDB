import "./App.css";

import Message from "./module_bindings/message";
import SendMessageReducer from "./module_bindings/send_message_reducer";
import SetNameReducer from "./module_bindings/set_name_reducer";
import User from "./module_bindings/user";

import { Identity, SpacetimeDBClient } from "@clockworklabs/spacetimedb-sdk";
import React, { useEffect, useRef, useState } from "react";

// Register the tables and reducers before creating the SpacetimeDBClient
SpacetimeDBClient.registerTables(Message, User);
SpacetimeDBClient.registerReducers(SendMessageReducer, SetNameReducer);

export type MessageType = {
  name: string;
  message: string;
};

const token = localStorage.getItem("auth_token") || undefined;
const spacetimeDBClient = new SpacetimeDBClient(
  "ws://localhost:3000",
  "chat",
  token
);

function App() {
  const [newName, setNewName] = useState("");
  const [settingName, setSettingName] = useState(false);
  const [name, setName] = useState("");
  const [systemMessage, setSystemMessage] = useState("");
  const [messages, setMessages] = useState<MessageType[]>([]);

  const [newMessage, setNewMessage] = useState("");

  const local_identity = useRef<Identity | undefined>(undefined);
  const initialized = useRef<boolean>(false);

  const client = useRef<SpacetimeDBClient>(spacetimeDBClient);
  client.current.on("disconnected", () => {
    console.log("disconnected");
  });
  client.current.on("client_error", () => {
    console.log("client_error");
  });

  client.current.onConnect((token: string, identity: Identity) => {
    console.log("Connected to SpacetimeDB");

    local_identity.current = identity;

    localStorage.setItem("auth_token", token);

    client.current.subscribe(["SELECT * FROM User", "SELECT * FROM Message"]);
  });

  function userNameOrIdentity(user: User): string {
    console.log(`Name: ${user.name} `);
    if (user.name !== null) return user.name || "";

    const identityStr = user.identity.toHexString();
    console.log(`Name: ${identityStr} `);
    return user.identity.toHexString().substring(0, 8);
  }

  function setAllMessagesInOrder() {
    const messages = Array.from(Message.all());
    messages.sort((a, b) => (a.sent > b.sent ? 1 : a.sent < b.sent ? -1 : 0));

    const messagesType: MessageType[] = messages.map((message) => {
      const sender = User.filterByIdentity(message.sender);
      const name = sender ? userNameOrIdentity(sender) : "unknown";

      return {
        name: name, // convert sender Uint8Array to name string using helper function
        message: message.text, // map text to message
      };
    });

    setMessages(messagesType);
  }

  client.current.on("initialStateSync", () => {
    setAllMessagesInOrder();
    const user = User.filterByIdentity(local_identity?.current!);

    if (user) {
      setName(userNameOrIdentity(user));
    }
  });

  // Helper function to append a line to the systemMessage state
  function appendToSystemMessage(line: string) {
    setSystemMessage((prevMessage) => `${prevMessage}\n${line}`);
  }

  User.onInsert((user) => {
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

  SendMessageReducer.on((reducerEvent) => {
    if (
      local_identity.current &&
      reducerEvent.callerIdentity.isEqual(local_identity.current)
    ) {
      if (reducerEvent.status === "failed") {
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
      if (reducerEvent.status === "failed") {
        appendToSystemMessage(`Error setting name: ${reducerEvent.message} `);
      } else if (reducerEvent.status === "committed") {
        setName(reducerArgs[0]);
      }
    }
  });

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    SetNameReducer.call(newName);
    setSettingName(false);
  };

  const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // send message here
    SendMessageReducer.call(newMessage);
    setNewMessage("");
  };

  useEffect(() => {
    if (!initialized.current) {
      client.current.connect();
      initialized.current = true;
    }
  }, []);

  return (
    <div className="App">
      <div className="profile">
        <h1>Profile</h1>
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
              Edit Name
            </button>
          </>
        ) : (
          <form onSubmit={onSubmitNewName}>
            <input
              type="text"
              style={{ marginBottom: "1rem" }}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
            <button type="submit">Submit</button>
          </form>
        )}
      </div>
      <div className="message">
        <h1>Messages</h1>
        {messages.length < 1 && <p>No messages</p>}
        <div>
          {messages.map((message) => (
            <div key={message.message}>
              <p>
                <b>{message.name}</b>: {message.message}
              </p>
            </div>
          ))}
        </div>
      </div>
      <div className="system" style={{ whiteSpace: "pre-wrap" }}>
        <h1>System</h1>
        <div>
          <p>{systemMessage}</p>
        </div>
      </div>
      <div className="new-message">
        <form
          onSubmit={onMessageSubmit}
          style={{
            display: "flex",
            flexDirection: "column",
            width: "50%",
            margin: "0 auto",
          }}
        >
          <h3>New Message</h3>
          <textarea
            value={newMessage}
            onChange={(e) => setNewMessage(e.target.value)}
          />
          <button type="submit">Send</button>
        </form>
      </div>
    </div>
  );
}

export default App;
