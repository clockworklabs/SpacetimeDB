# Tutorial: Auth Tokens, Keychain, and Reconnect

## Goal

Persist auth tokens across launches and configure resilient reconnect behavior.

## 1. Configure a token store

```swift
let tokenStore = KeychainTokenStore(service: "com.example.myapp.spacetimedb")
```

## 2. Reuse stored token on connect

```swift
let savedToken = tokenStore.load(forModule: "my-module")
client.connect(token: savedToken)
```

## 3. Save token when identity is received

```swift
func onIdentityReceived(identity: [UInt8], token: String) {
    tokenStore.save(token: token, forModule: "my-module")
}
```

## 4. Tune reconnect policy

```swift
let policy = ReconnectPolicy(
    maxRetries: nil,
    initialDelaySeconds: 1.0,
    maxDelaySeconds: 30.0,
    multiplier: 2.0,
    jitterRatio: 0.2
)
```

## 5. Create client with reconnect and compression config

```swift
let client = SpacetimeClient(
    serverUrl: URL(string: "http://127.0.0.1:3000")!,
    moduleName: "my-module",
    reconnectPolicy: policy,
    compressionMode: .gzip
)
```

`SpacetimeClient` includes connectivity monitoring and defers reconnect attempts while offline.

## Next

- [Publishing DocC and Submitting to Swift Package Index](doc:Publishing-and-Swift-Package-Index)
