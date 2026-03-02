# ``SpacetimeDB``

Native Swift SDK for SpacetimeDB realtime clients on Apple platforms.

## Overview

`SpacetimeDB` provides:

- BSATN protocol encoding/decoding
- WebSocket transport and reconnect support
- Typed table cache integration
- Reducer/procedure/query client APIs
- Apple platform support for macOS and iOS

## Key Types

- ``SpacetimeClient``
- ``SpacetimeClientDelegate``
- ``ReconnectPolicy``
- ``CompressionMode``
- ``SubscriptionHandle``
- ``ClientCache``
- ``TableCache``
- ``KeychainTokenStore``

## Tutorials

- [Tutorial: Build Your First SpacetimeDB Swift Client](doc:Tutorial-Build-First-Client)
- [Tutorial: Auth Tokens, Keychain, and Reconnect](doc:Tutorial-Auth-Reconnect)

## Publishing

- [Publishing DocC and Submitting to Swift Package Index](doc:Publishing-and-Swift-Package-Index)
- [Apple CI Matrix (macOS, iOS Simulator)](doc:Apple-CI-Matrix)
