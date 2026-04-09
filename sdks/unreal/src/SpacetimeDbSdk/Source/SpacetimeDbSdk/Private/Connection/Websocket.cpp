
#include "Connection/Websocket.h"
#include "WebSocketsModule.h" // Required for FWebSocketsModule
#include "SpacetimeDbSdk/Public/BSATN/UESpacetimeDB.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"

#include "Dom/JsonObject.h"
#include "Serialization/JsonWriter.h"
#include "Serialization/JsonSerializer.h"

namespace
{
const FString V2Protocol = TEXT("v2.bsatn.spacetimedb");
const FString V3Protocol = TEXT("v3.bsatn.spacetimedb");

const FString& GetProtocolName(ESpacetimeDBWsProtocol Protocol)
{
	return Protocol == ESpacetimeDBWsProtocol::V3 ? V3Protocol : V2Protocol;
}
}

UWebsocketManager::UWebsocketManager()
{
	// Ensure the WebSockets module is loaded.
	FModuleManager::LoadModuleChecked<FWebSocketsModule>(TEXT("WebSockets"));
}

void UWebsocketManager::BeginDestroy()
{
	UE_LOG(LogSpacetimeDb_Connection, Log, TEXT("UWebsocketManager::BeginDestroy: Cleaning up WebSocket."));
	if (!HasAnyFlags(RF_ClassDefaultObject))
	{
		Disconnect();
	}
	Super::BeginDestroy();
}

void UWebsocketManager::Connect(const FString& ServerUrl)
{
	if (IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("UWebsocketManager::Connect: Already connected. Disconnect first."));
		return;
	}

	if (ServerUrl.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UWebsocketManager::Connect called with empty URL"));
		OnConnectionError.Broadcast(TEXT("Invalid server URL"));
		return;
	}

	PendingServerUrl = ServerUrl;
	bHasEstablishedConnection = false;
	bHasAttemptedV2Fallback = false;
	// Unreal's websocket API accepts one subprotocol string per connection, so
	// we prefer v3 first and retry with v2 only if the initial handshake fails.
	ConnectWithProtocol(ServerUrl, ESpacetimeDBWsProtocol::V3);
}

void UWebsocketManager::Disconnect()
{
	if (!WebSocket.IsValid())
	{
		PendingServerUrl.Empty();
		bHasEstablishedConnection = false;
		bHasAttemptedV2Fallback = false;
		return;
	}

	if (IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Log, TEXT("UWebsocketManager::Disconnect: Closing WebSocket connection."));
		WebSocket->Close();
	}

	PendingServerUrl.Empty();
	bHasEstablishedConnection = false;
	bHasAttemptedV2Fallback = false;
	// Reset the WebSocket to allow for reconnection attempts
	ResetSocket();
}

bool UWebsocketManager::SendMessage(const FString& Message)
{
	if (!IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("UWebsocketManager::SendMessage: WebSocket is not connected."));
		return false;
	}

	if (!WebSocket.IsValid())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UWebsocketManager::SendMessage: WebSocket is not valid."));
		return false;
	}

	// send the message as a UTF-8 encoded string
	WebSocket->Send(Message);
	return true;
}

bool UWebsocketManager::SendMessage(const TArray<uint8>& Data)
{
	if (!IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("UWebsocketManager::SendMessage: WebSocket is not connected."));
		return false;
	}

	if (!WebSocket.IsValid())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UWebsocketManager::SendMessage: WebSocket is not valid."));
		return false;
	}

	// send the data as a binary message
	WebSocket->Send(Data.GetData(), Data.Num(), true);
	return true;
}

bool UWebsocketManager::IsConnected() const
{
	return WebSocket.IsValid() && WebSocket->IsConnected();
}

void UWebsocketManager::SetInitToken(FString Token)
{
	InitToken = Token;
}

void UWebsocketManager::HandleConnected()
{
	bHasEstablishedConnection = true;
	UE_LOG(
		LogSpacetimeDb_Connection,
		Log,
		TEXT("UWebsocketManager: WebSocket Connected using %s."),
		*GetProtocolName(ActiveProtocol)
	);
	OnConnected.Broadcast();
}

void UWebsocketManager::HandleConnectionError(const FString& Error)
{
	if (TryFallbackToV2(Error))
	{
		return;
	}
	UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UWebsocketManager: WebSocket Connection Error: %s"), *Error);
	OnConnectionError.Broadcast(Error);
	// Reset on error to allow reconnection attempts
	ResetSocket();
}

void UWebsocketManager::HandleMessageReceived(const FString& Message)
{
	OnMessageReceived.Broadcast(Message);
}

void UWebsocketManager::HandleBinaryMessageReceived(const void* Data, SIZE_T Size, bool bIsLastFragment)
{
	if (Size == 0)
	{
		return;
	}

	// Handle binary messages, which may be fragmented
	const uint8* Bytes = static_cast<const uint8*>(Data);

	// Append this fragment to our buffer
	IncompleteMessage.Append(Bytes, Size);

	// If this is the last fragment, we have the complete message
	if (bIsLastFragment)
	{
		// We have the complete message
		TArray<uint8> MessageBytes = IncompleteMessage;
		IncompleteMessage.Reset();
		bAwaitingBinaryFragments = false;

		// Forward the complete binary payload to listeners.
		OnBinaryMessageReceived.Broadcast(MessageBytes);
	}
	else
	{
		// More fragments are coming
		bAwaitingBinaryFragments = true;
	}
}

void UWebsocketManager::HandleClosed(int32 StatusCode, const FString& Reason, bool bWasClean)
{
	if (TryFallbackToV2(Reason))
	{
		return;
	}
	UE_LOG(LogSpacetimeDb_Connection, Log, TEXT("UWebsocketManager: WebSocket Closed. Status: %d, Reason: %s, Clean: %s"),
		StatusCode, *Reason, bWasClean ? TEXT("true") : TEXT("false"));
	// Notify listeners about the closure
	OnClosed.Broadcast(StatusCode, Reason, bWasClean);
	// Reset on close to allow reconnection attempts
	ResetSocket();
}

void UWebsocketManager::ConnectWithProtocol(const FString& ServerUrl, ESpacetimeDBWsProtocol Protocol)
{
	ActiveProtocol = Protocol;
	++ConnectAttemptId;
	const uint32 AttemptId = ConnectAttemptId;

	TMap<FString, FString> UpgradeHeaders;
	if (!InitToken.IsEmpty())
	{
		const FString HeaderToken = FString::Printf(TEXT("Bearer %s"), *InitToken);
		UpgradeHeaders.Add(TEXT("Authorization"), HeaderToken);
	}

	WebSocket = FWebSocketsModule::Get().CreateWebSocket(ServerUrl, GetProtocolName(Protocol), UpgradeHeaders);
	if (!WebSocket.IsValid())
	{
		UE_LOG(
			LogSpacetimeDb_Connection,
			Error,
			TEXT("UWebsocketManager::Connect: Failed to create WebSocket connection to %s."),
			*ServerUrl
		);
		if (TryFallbackToV2(TEXT("failed to create websocket")))
		{
			return;
		}
		OnConnectionError.Broadcast(TEXT("Failed to create WebSocket."));
		return;
	}

	const TWeakObjectPtr<UWebsocketManager> WeakThis(this);
	WebSocket->OnConnected().AddLambda([WeakThis, AttemptId]()
	{
		UWebsocketManager* This = WeakThis.Get();
		if (!This || This->ConnectAttemptId != AttemptId)
		{
			return;
		}
		This->HandleConnected();
	});
	WebSocket->OnConnectionError().AddLambda([WeakThis, AttemptId](const FString& Error)
	{
		UWebsocketManager* This = WeakThis.Get();
		if (!This || This->ConnectAttemptId != AttemptId)
		{
			return;
		}
		This->HandleConnectionError(Error);
	});
	WebSocket->OnMessage().AddLambda([WeakThis, AttemptId](const FString& Message)
	{
		UWebsocketManager* This = WeakThis.Get();
		if (!This || This->ConnectAttemptId != AttemptId)
		{
			return;
		}
		This->HandleMessageReceived(Message);
	});
	WebSocket->OnBinaryMessage().AddLambda([WeakThis, AttemptId](const void* Data, SIZE_T Size, bool bIsLastFragment)
	{
		UWebsocketManager* This = WeakThis.Get();
		if (!This || This->ConnectAttemptId != AttemptId)
		{
			return;
		}
		This->HandleBinaryMessageReceived(Data, Size, bIsLastFragment);
	});
	WebSocket->OnClosed().AddLambda([WeakThis, AttemptId](int32 StatusCode, const FString& Reason, bool bWasClean)
	{
		UWebsocketManager* This = WeakThis.Get();
		if (!This || This->ConnectAttemptId != AttemptId)
		{
			return;
		}
		This->HandleClosed(StatusCode, Reason, bWasClean);
	});

	UE_LOG(
		LogSpacetimeDb_Connection,
		Log,
		TEXT("UWebsocketManager::Connect: Connecting to %s with %s..."),
		*ServerUrl,
		*GetProtocolName(Protocol)
	);
	WebSocket->Connect();
}

bool UWebsocketManager::TryFallbackToV2(const FString& FailureReason)
{
	// Only downgrade during the initial connect path. Once a websocket session
	// has been established we preserve the chosen transport version across later
	// disconnect/error handling instead of silently switching protocols.
	if (bHasEstablishedConnection || bHasAttemptedV2Fallback || ActiveProtocol != ESpacetimeDBWsProtocol::V3 || PendingServerUrl.IsEmpty())
	{
		return false;
	}

	bHasAttemptedV2Fallback = true;
	UE_LOG(
		LogSpacetimeDb_Connection,
		Warning,
		TEXT("v3 websocket connection failed (%s). Retrying with %s."),
		*FailureReason,
		*GetProtocolName(ESpacetimeDBWsProtocol::V2)
	);
	ResetSocket();
	ConnectWithProtocol(PendingServerUrl, ESpacetimeDBWsProtocol::V2);
	return true;
}

void UWebsocketManager::ResetSocket()
{
	IncompleteMessage.Reset();
	bAwaitingBinaryFragments = false;
	WebSocket.Reset();
}
