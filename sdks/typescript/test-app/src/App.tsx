import React, { useEffect, useState } from "react";
import { SpacetimeDBClient } from "@clockworklabs/spacetimedb-typescript-sdk";
import "./App.css";

function App() {
  const [client] = useState<SpacetimeDBClient>(
    new SpacetimeDBClient("localhost:3000", "goldbreezycanid")
  );

  useEffect(() => {
    if (client.live) {
      client.connect();
    }
  }, [client]);

  return (
    <div className="App">
      <h1>Typescript SDK Test!</h1>
    </div>
  );
}

export default App;
