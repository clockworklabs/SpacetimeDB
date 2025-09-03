# Connection

This folder contains the classes used to open and manage a connection to a SpacetimeDB server.  They expose Blueprint friendly APIs for creating a websocket, authenticating and managing active subscriptions.

## Files

- `Callback.h` – Defines `UStatus` and related enums used to report reducer results and statuses to the user.
- `Credentials.h` – Static helper functions for persisting authentication tokens via Unreal's config system.
- `DbConnectionBase.h` – Core connection object. Handles websocket events, table caches and reducer calls. Used as a base class for generated `DbConnection` class.
- `DbConnectionBuilder.h` – Fluent builder used to configure a connection instance and bind event delegates. Used as a base class for generated `DbConnectionBuilder` class.
- `SetReducerFlags.h` – Container for flags controlling reducer call behaviour (e.g. disabling/enabling success notifications).
- `Subscription.h` – Classes for constructing and managing query subscriptions.
- `Websocket.h` – Wrapper around UE's `IWebSocket` that sends/receives messages.