import React, { useEffect, useState } from "react";
import logo from "./logo.svg";
import "./App.css";

import { SpacetimeDBClient } from "@clockworklabs/spacetimedb-sdk";

export type MessageType = {
  name: string;
  message: string;
};

function App() {
  const [newName, setNewName] = useState("");
  const [settingName, setSettingName] = useState(false);
  const [name, setName] = useState("DerekCW");
  const [messages, setMessages] = useState<MessageType[]>([]);

  const [newMessage, setNewMessage] = useState("");

  const [client] = useState<SpacetimeDBClient>(
    new SpacetimeDBClient("wss://localhost:3000", "chat")
  );

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setName(newName);
    setSettingName(false);
  };

  const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // send message here
    setNewMessage("");
  };

  useEffect(() => {
    client.connect();
    client.on("disconnected", () => {
      console.log("disconnected");
    });
    client.on("client_error", () => {
      console.log("client_error");
    });

    client.on("connected", (e) => {
      // logs the identity
      console.log(e);
    });
  }, [client]);

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
          {messages.map((message, key) => (
            <div key={key}>
              <p>
                <b>{message.name}</b>: {message.message}
              </p>
            </div>
          ))}
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
          ></textarea>
          <button type="submit">Send</button>
        </form>
      </div>
    </div>
  );
}

export default App;
