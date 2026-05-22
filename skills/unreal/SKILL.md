---
name: unreal
description: SpacetimeDB Unreal Engine client SDK reference. Use when building Unreal Engine clients that connect to SpacetimeDB.
license: Apache-2.0
metadata:
  author: clockworklabs
  version: "2.0"
  role: client
  language: cpp
  cursor_globs: "**/*.cpp,**/*.h"
  cursor_always_apply: true
---

# SpacetimeDB Unreal Engine Integration

This skill covers Unreal Engine-specific patterns for connecting to SpacetimeDB. For server-side module development, see the `rust-server` or `csharp-server` skills.

---

## Installation

Add the SpacetimeDB Unreal SDK as a plugin:

1. Create a `Plugins` folder in your Unreal project root if it does not exist.
2. Copy the `SpacetimeDbSdk` folder into `Plugins/`.
3. Right-click your `.uproject` file and select **Generate Visual Studio project files**.
4. Add `"SpacetimeDbSdk"` to your module's `Build.cs`:

```csharp
PublicDependencyModuleNames.AddRange(new string[] { "SpacetimeDbSdk" });
```

---

## Generate Module Bindings

```bash
spacetime generate --lang unrealcpp \
  --uproject-dir <path_to_uproject_directory> \
  --module-path <path_to_spacetimedb_module> \
  --unreal-module-name <your_unreal_module_name>
```

This generates C++ bindings in `ModuleBindings/` inside your project. Include the generated header:

```cpp
#include "ModuleBindings/SpacetimeDBClient.g.h"
```

Regenerate whenever you change module tables, reducers, or types.

---

## GameManager Actor Pattern

The recommended pattern is a singleton Actor that owns the connection. Enable ticking so `FrameTick` is called every frame.

### Header (GameManager.h)

```cpp
#pragma once
#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"
#include "GameManager.generated.h"

class UDbConnection;

UCLASS()
class AGameManager : public AActor
{
    GENERATED_BODY()
public:
    AGameManager();
    static AGameManager* Instance;

    UPROPERTY(EditAnywhere, Category="SpacetimeDB")
    FString ServerUri = TEXT("127.0.0.1:3000");

    UPROPERTY(EditAnywhere, Category="SpacetimeDB")
    FString DatabaseName = TEXT("my-module");

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    UDbConnection* Conn = nullptr;

    UPROPERTY(BlueprintReadOnly, Category="SpacetimeDB")
    FSpacetimeDBIdentity LocalIdentity;

protected:
    virtual void BeginPlay() override;
    virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;
public:
    virtual void Tick(float DeltaTime) override;

private:
    UFUNCTION() void HandleConnect(UDbConnection* InConn, FSpacetimeDBIdentity Identity, const FString& Token);
    UFUNCTION() void HandleConnectError(const FString& Error);
    UFUNCTION() void HandleDisconnect(UDbConnection* InConn, const FString& Error);
    UFUNCTION() void HandleSubscriptionApplied(FSubscriptionEventContext& Context);
};
```

### Source (GameManager.cpp)

```cpp
#include "GameManager.h"
#include "Connection/Credentials.h"

AGameManager* AGameManager::Instance = nullptr;

AGameManager::AGameManager()
{
    PrimaryActorTick.bCanEverTick = true;
    PrimaryActorTick.bStartWithTickEnabled = true;
}

void AGameManager::BeginPlay()
{
    Super::BeginPlay();
    Instance = this;

    FOnConnectDelegate ConnectDelegate;
    BIND_DELEGATE_SAFE(ConnectDelegate, this, AGameManager, HandleConnect);
    FOnDisconnectDelegate DisconnectDelegate;
    BIND_DELEGATE_SAFE(DisconnectDelegate, this, AGameManager, HandleDisconnect);
    FOnConnectErrorDelegate ConnectErrorDelegate;
    BIND_DELEGATE_SAFE(ConnectErrorDelegate, this, AGameManager, HandleConnectError);

    UCredentials::Init(TEXT(".spacetime_token"));
    FString Token = UCredentials::LoadToken();

    UDbConnectionBuilder* Builder = UDbConnection::Builder()
        ->WithUri(ServerUri)
        ->WithDatabaseName(DatabaseName)
        ->OnConnect(ConnectDelegate)
        ->OnDisconnect(DisconnectDelegate)
        ->OnConnectError(ConnectErrorDelegate);

    if (!Token.IsEmpty())
    {
        Builder->WithToken(Token);
    }

    Conn = Builder->Build();
}

void AGameManager::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
    if (Conn) { Conn->Disconnect(); Conn = nullptr; }
    if (Instance == this) { Instance = nullptr; }
    Super::EndPlay(EndPlayReason);
}

void AGameManager::Tick(float DeltaTime)
{
    if (Conn && Conn->IsActive())
    {
        Conn->FrameTick();
    }
}

void AGameManager::HandleConnect(UDbConnection* InConn, FSpacetimeDBIdentity Identity, const FString& Token)
{
    LocalIdentity = Identity;
    UCredentials::SaveToken(Token);

    FOnSubscriptionApplied AppliedDelegate;
    BIND_DELEGATE_SAFE(AppliedDelegate, this, AGameManager, HandleSubscriptionApplied);
    Conn->SubscriptionBuilder()
        ->OnApplied(AppliedDelegate)
        ->SubscribeToAllTables();
}

void AGameManager::HandleConnectError(const FString& Error)
{
    UE_LOG(LogTemp, Error, TEXT("Connection error: %s"), *Error);
}

void AGameManager::HandleDisconnect(UDbConnection* InConn, const FString& Error)
{
    UE_LOG(LogTemp, Warning, TEXT("Disconnected: %s"), *Error);
}

void AGameManager::HandleSubscriptionApplied(FSubscriptionEventContext& Context)
{
    UE_LOG(LogTemp, Log, TEXT("Subscription applied - game state loaded"));
}
```

---

## FrameTick -- Critical

**You must either call `Conn->FrameTick()` every frame in your Actor's `Tick()`, or call `Conn->SetAutoTicking(true)` once at startup.** The SDK queues all network messages and only processes them on tick. Without one of these, no callbacks fire and the client appears frozen.

---

## Connection Builder

Build a connection with the builder pattern. All builder methods return pointers for chaining with `->`.

```cpp
UDbConnection* Conn = UDbConnection::Builder()
    ->WithUri(TEXT("127.0.0.1:3000"))
    ->WithDatabaseName(TEXT("my-module"))
    ->WithToken(SavedToken)                              // optional
    ->WithCompression(ESpacetimeDBCompression::Gzip)     // optional
    ->OnConnect(ConnectDelegate)
    ->OnConnectError(ErrorDelegate)
    ->OnDisconnect(DisconnectDelegate)
    ->Build();
```

### OnConnect callback signature

```cpp
UFUNCTION()
void OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token);
```

Save the `Token` for future reconnection. The `Identity` is the user's persistent identifier.

---

## Subscribing to Tables

After connecting, subscribe to receive table data:

```cpp
// Subscribe to all public tables
Conn->SubscriptionBuilder()
    ->OnApplied(AppliedDelegate)
    ->SubscribeToAllTables();

// Subscribe to specific queries
TArray<FString> Queries = { TEXT("SELECT * FROM player"), TEXT("SELECT * FROM entity") };
Conn->SubscriptionBuilder()
    ->OnApplied(AppliedDelegate)
    ->OnError(ErrorDelegate)
    ->Subscribe(Queries);
```

### Subscription Handle

`Subscribe` and `SubscribeToAllTables` return a `USubscriptionHandle*`:

```cpp
USubscriptionHandle* Handle = Conn->SubscriptionBuilder()->...->Subscribe(Queries);
Handle->IsActive();      // true while subscription is live
Handle->Unsubscribe();   // cancel the subscription
Handle->UnsubscribeThen(OnEndDelegate); // cancel with callback
Handle->GetQuerySqls();  // get the SQL queries
```

---

## Reading the Client Cache

Access tables through `Conn->Db`:

```cpp
// Find by unique/primary key (returns by value; default-constructed if not found)
FUserType User = Conn->Db->User->Identity->Find(SomeIdentity);

// Filter by BTree index
TArray<FPlayerType> LevelFive = Conn->Db->Player->Level->Filter(5);

// Iterate all rows
TArray<FEntityType> AllEntities = Conn->Db->Entity->Iter();

// Count
int32 Total = Conn->Db->Player->Count();
```

---

## Row Callbacks

Register callbacks on table objects. Callbacks use Unreal dynamic multicast delegates.

```cpp
// OnInsert
Conn->Db->User->OnInsert.AddDynamic(this, &AMyActor::OnUserInsert);

// OnDelete
Conn->Db->User->OnDelete.AddDynamic(this, &AMyActor::OnUserDelete);

// OnUpdate (only fires for rows with a primary key)
Conn->Db->User->OnUpdate.AddDynamic(this, &AMyActor::OnUserUpdate);
```

### Callback signatures (must be UFUNCTION)

```cpp
UFUNCTION()
void OnUserInsert(const FEventContext& Context, const FUserType& NewRow);

UFUNCTION()
void OnUserDelete(const FEventContext& Context, const FUserType& DeletedRow);

UFUNCTION()
void OnUserUpdate(const FEventContext& Context, const FUserType& OldRow, const FUserType& NewRow);
```

Register callbacks before connecting or in `HandleSubscriptionApplied`.

---

## Calling Reducers

Invoke reducers through `Conn->Reducers`:

```cpp
Conn->Reducers->SendMessage(TEXT("Hello!"));
Conn->Reducers->SetName(TEXT("Alice"));
Conn->Reducers->MovePlayer(1.0f, 0.0f);
```

### Reducer Result Callbacks

Observe when a reducer you called completes:

```cpp
Conn->Reducers->OnSendMessage.AddDynamic(this, &AMyActor::OnSendMessageResult);
```

```cpp
UFUNCTION()
void OnSendMessageResult(const FReducerEventContext& Context, const FString& Text)
{
    UE_LOG(LogTemp, Log, TEXT("SendMessage result for: %s"), *Text);
}
```

These delegates fire only for reducer calls made by this connection, not for other clients' calls.

---

## Delegate Binding with BIND_DELEGATE_SAFE

Use the `BIND_DELEGATE_SAFE` macro to safely bind delegates to member functions:

```cpp
FOnConnectDelegate ConnectDelegate;
BIND_DELEGATE_SAFE(ConnectDelegate, this, AMyActor, HandleConnect);
```

This is the recommended pattern for all SpacetimeDB delegate bindings in C++.

---

## Identity and ConnectionId

```cpp
// FSpacetimeDBIdentity -- 256-bit unique user identifier, persists across connections
FSpacetimeDBIdentity Identity;
Identity.ToHex();

// FSpacetimeDBConnectionId -- 128-bit per-session connection identifier
FSpacetimeDBConnectionId ConnId = Conn->GetConnectionId();

// From any context
FSpacetimeDBIdentity Id;
bool Found = Context.TryGetIdentity(Id);
FSpacetimeDBConnectionId CId = Context.GetConnectionId();
```

---

## Token Persistence

Use the built-in `UCredentials` helper to save and load tokens:

```cpp
UCredentials::Init(TEXT(".spacetime_token"));
FString Token = UCredentials::LoadToken();
// ... after connect:
UCredentials::SaveToken(Token);
```

---

## Context Types

All callbacks receive a context struct that provides access to `Db` and `Reducers`:

| Type | Used In |
|------|---------|
| `FEventContext` | Table row callbacks (OnInsert, OnDelete, OnUpdate) |
| `FReducerEventContext` | Reducer result callbacks |
| `FSubscriptionEventContext` | Subscription lifecycle callbacks (OnApplied, OnError) |
| `FErrorContext` | Error callbacks |

All inherit from `FContextBase` which provides:

```cpp
Context.Db          // URemoteTables* -- client cache
Context.Reducers    // URemoteReducers* -- invoke reducers
Context.SubscriptionBuilder()  // start a new subscription
```

---

## Blueprint Integration

All core classes are Blueprint-accessible via `UFUNCTION(BlueprintCallable)` and `UPROPERTY(BlueprintReadOnly/BlueprintAssignable)`:

- `UDbConnection::Builder()` and all builder methods are `BlueprintCallable`.
- Table callbacks (`OnInsert`, `OnDelete`, `OnUpdate`) are `BlueprintAssignable` delegates.
- Reducer invoke methods and result delegates are Blueprint-accessible.
- `Conn->Db` and `Conn->Reducers` are `BlueprintReadOnly` properties.
- Generated row types are `BlueprintType` USTRUCTs with `BlueprintReadWrite` properties.

This means you can build the entire connection and callback flow in Blueprints without writing C++.

---

## Unreal-Specific Considerations

### Auto Ticking Alternative

`UDbConnection` inherits from `FTickableGameObject`, but auto ticking is **off by default**. You have two options:

```cpp
// Option 1: Call FrameTick() manually in your Actor's Tick() (shown in GameManager above)
void Tick(float DeltaTime) { Conn->FrameTick(); }

// Option 2: Enable auto ticking. The SDK then processes messages every frame automatically
Conn->SetAutoTicking(true);
```

Pick one. Without either, no callbacks fire.

### Compression

```cpp
Builder->WithCompression(ESpacetimeDBCompression::Gzip);  // default
Builder->WithCompression(ESpacetimeDBCompression::None);   // no compression
```

### Generated Types

Codegen produces USTRUCTs prefixed with `F` (e.g., `FUserType`, `FEntityType`) and table classes prefixed with `U` (e.g., `UUserTable`). Row types use `GENERATED_BODY()` and `UPROPERTY()` for full reflection support.
