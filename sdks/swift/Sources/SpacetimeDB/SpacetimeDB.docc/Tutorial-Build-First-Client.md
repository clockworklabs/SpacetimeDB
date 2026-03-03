# Tutorial: Build Your First SpacetimeDB Swift Client

## Goal

Create a minimal client that connects, subscribes, and reacts to updates.

## 1. Add the package

```swift
dependencies: [
    .package(url: "https://github.com/<org>/spacetimedb-swift.git", from: "0.1.0"),
]
```

Add target dependency:

```swift
.product(name: "SpacetimeDB", package: "spacetimedb-swift")
```

## 2. Register generated tables

```swift
SpacetimeModule.registerTables()
```

## 3. Create and connect a client

```swift
let client = SpacetimeClient(
    serverUrl: URL(string: "http://127.0.0.1:3000")!,
    moduleName: "my-module"
)
client.delegate = self
client.connect()
```

## 4. Subscribe to data

Assuming `import os` is present in the file:

```swift
let logger = Logger(subsystem: "com.example.myapp", category: "SpacetimeClient")

let handle = client.subscribe(
    queries: ["SELECT * FROM person"],
    onApplied: { logger.info("Initial snapshot applied") },
    onError: { message in
        logger.error("Subscription failed: \(message, privacy: .public)")
    }
)
```

## 5. Read replicated rows

```swift
let people = PersonTable.cache.rows
```

## 6. Send reducers/procedures

```swift
Add.invoke(name: "Avi")
let result: String = try await client.sendProcedure("say_hello", Data(), responseType: String.self)
```

## 7. Disconnect cleanly

```swift
handle.unsubscribe()
client.disconnect()
```

## Next

- [Tutorial: Auth Tokens, Keychain, and Reconnect](doc:Tutorial-Auth-Reconnect)
