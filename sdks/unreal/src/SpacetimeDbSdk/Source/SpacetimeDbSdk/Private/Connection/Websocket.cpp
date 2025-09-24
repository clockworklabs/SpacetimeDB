
#include "Connection/Websocket.h"
#include "WebSocketsModule.h" // Required for FWebSocketsModule
#include "SpacetimeDbSdk/Public/BSATN/UESpacetimeDB.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/CompressableQueryUpdateType.g.h"
#include "Misc/Compression.h"

#include "Dom/JsonObject.h"
#include "Serialization/JsonWriter.h"
#include "Serialization/JsonSerializer.h"

static void LogIdentityTokenHex(const FIdentityTokenType& InToken, const TCHAR* TagName)
{
	// Logs the identity token in a structured format for debugging purposes.
	TSharedRef<FJsonObject> Obj = MakeShared<FJsonObject>();
	Obj->SetStringField(TEXT("__identity__"), InToken.Identity.ToHex());
	Obj->SetStringField(TEXT("token"), InToken.Token);
	Obj->SetStringField(TEXT("__connection_id__"), InToken.ConnectionId.ToHex());

	FString Json;
	TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&Json);
	FJsonSerializer::Serialize(Obj, Writer);
	UE_LOG(LogTemp, Log, TEXT("[%s] %s"), TagName, *Json);
}

UWebsocketManager::UWebsocketManager()
{
	// Ensure the WebSockets module is loaded.
	FModuleManager::LoadModuleChecked<FWebSocketsModule>(TEXT("WebSockets"));
}

void UWebsocketManager::BeginDestroy()
{
	UE_LOG(LogTemp, Log, TEXT("UWebsocketManager::BeginDestroy: Cleaning up WebSocket."));
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
		UE_LOG(LogTemp, Warning, TEXT("UWebsocketManager::Connect: Already connected. Disconnect first."));
		return;
	}

	if (ServerUrl.IsEmpty())
	{
		UE_LOG(LogTemp, Error, TEXT("UWebsocketManager::Connect called with empty URL"));
		OnConnectionError.Broadcast(TEXT("Invalid server URL"));
		return;
	}

	// append InitToken to the connection headers if provided
	TMap<FString, FString> UpgradeHeaders;
	if (!InitToken.IsEmpty())
	{
		FString HeaderToken = FString::Printf(TEXT("Bearer %s"),
			*InitToken);
		UpgradeHeaders.Add("Authorization", HeaderToken);
	}

	// using the v1.bsatn.spacetimedb protocol for WebSocket connections
	const FString Protocol = "v1.bsatn.spacetimedb"; // @TODO: Implement JSON alternative, v1.json.spacetimedb

	// Create the WebSocket connection
	WebSocket = FWebSocketsModule::Get().CreateWebSocket(ServerUrl, Protocol, UpgradeHeaders);

	if (!WebSocket.IsValid())
	{
		UE_LOG(LogTemp, Error, TEXT("UWebsocketManager::Connect: Failed to create WebSocket connection to %s."), *ServerUrl);
		OnConnectionError.Broadcast(TEXT("Failed to create WebSocket."));
		return;
	}

	// Bind event handlers
	WebSocket->OnConnected().AddUObject(this, &UWebsocketManager::HandleConnected);
	WebSocket->OnConnectionError().AddUObject(this, &UWebsocketManager::HandleConnectionError);
	WebSocket->OnMessage().AddUObject(this, &UWebsocketManager::HandleMessageReceived);
	WebSocket->OnRawMessage().AddUObject(this, &UWebsocketManager::HandleBinaryMessageReceived);
	WebSocket->OnClosed().AddUObject(this, &UWebsocketManager::HandleClosed);

	UE_LOG(LogTemp, Log, TEXT("UWebsocketManager::Connect: Connecting to %s..."), *ServerUrl);
	// Start the connection process
	WebSocket->Connect();
}

void UWebsocketManager::Disconnect()
{
	if (!WebSocket.IsValid())
	{
		return;
	}

	if (IsConnected())
	{
		UE_LOG(LogTemp, Log, TEXT("UWebsocketManager::Disconnect: Closing WebSocket connection."));
		WebSocket->Close();
	}

	// Reset the WebSocket to allow for reconnection attempts
	WebSocket.Reset();
}

bool UWebsocketManager::SendMessage(const FString& Message)
{
	if (!IsConnected())
	{
		UE_LOG(LogTemp, Warning, TEXT("UWebsocketManager::SendMessage: WebSocket is not connected."));
		return false;
	}

	if (!WebSocket.IsValid())
	{
		UE_LOG(LogTemp, Error, TEXT("UWebsocketManager::SendMessage: WebSocket is not valid."));
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
		UE_LOG(LogTemp, Warning, TEXT("UWebsocketManager::SendMessage: WebSocket is not connected."));
		return false;
	}

	if (!WebSocket.IsValid())
	{
		UE_LOG(LogTemp, Error, TEXT("UWebsocketManager::SendMessage: WebSocket is not valid."));
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
	UE_LOG(LogTemp, Log, TEXT("UWebsocketManager: WebSocket Connected."));
	OnConnected.Broadcast();
}

void UWebsocketManager::HandleConnectionError(const FString& Error)
{
	UE_LOG(LogTemp, Error, TEXT("UWebsocketManager: WebSocket Connection Error: %s"), *Error);
	OnConnectionError.Broadcast(Error);
	// Reset on error to allow reconnection attempts
	WebSocket.Reset(); 
}

void UWebsocketManager::HandleMessageReceived(const FString& Message)
{
	OnMessageReceived.Broadcast(Message);
}

void UWebsocketManager::HandleBinaryMessageReceived(const void* Data, SIZE_T Size, SIZE_T BytesRemaining)
{
	if (Size == 0)
	{
		return;
	}

	// Handle binary messages, which may be fragmented
	const uint8* Bytes = static_cast<const uint8*>(Data);

	if (IncompleteMessage.Num() > 0 && !bAwaitingBinaryFragments)
	{
		UE_LOG(LogTemp, Error, TEXT("Received binary fragment while previous data pending"));
	}

	// Append new incoming bytes to any incomplete message
	IncompleteMessage.Append(Bytes, Size);

	if (BytesRemaining > 0)
	{
		// Still expecting more fragments
		bAwaitingBinaryFragments = true;
		return;
	}

	// Final fragment received, reset and process
	bAwaitingBinaryFragments = false;

	TArray<uint8> MessageBytes = IncompleteMessage;
	IncompleteMessage.Reset();

	// Forward the complete binary payload to listeners.
	OnBinaryMessageReceived.Broadcast(MessageBytes);

}

void UWebsocketManager::HandleClosed(int32 StatusCode, const FString& Reason, bool bWasClean)
{
	UE_LOG(LogTemp, Log, TEXT("UWebsocketManager: WebSocket Closed. Status: %d, Reason: %s, Clean: %s"),
		StatusCode, *Reason, bWasClean ? TEXT("true") : TEXT("false"));
	// Notify listeners about the closure
	OnClosed.Broadcast(StatusCode, Reason, bWasClean);
	// Reset on close to allow reconnection attempts
	WebSocket.Reset(); 
}