#pragma once

#include "CoreMinimal.h"
#include "IWebSocket.h"
#include "ModuleBindings/Types/ServerMessageType.g.h"
#include "ModuleBindings/Types/CompressableQueryUpdateType.g.h"
#include "JsonObjectConverter.h" // for JSON debugging helpers
#include "Async/Async.h"
#include "HAL/CriticalSection.h"
#include "Misc/ScopeLock.h"


#include "Websocket.generated.h" 

/** Delegate broadcast when a connection is successfully established */
DECLARE_DYNAMIC_MULTICAST_DELEGATE(FOnWebSocketConnected);
/** Delegate broadcast on connection error */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnWebSocketConnectionError, const FString&, ErrorMessage);
/** Delegate broadcast when a text message is received */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnWebSocketMessageReceived, const FString&, Message);
/** Delegate broadcast when the socket closes */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_ThreeParams(FOnWebSocketClosed, int32, StatusCode, const FString&, Reason, bool, bWasClean);
/** Delegate broadcast when binary data is received */
DECLARE_DYNAMIC_MULTICAST_DELEGATE_OneParam(FOnWebSocketBinaryMessageReceived, const TArray<uint8>&, Data);


/**
 * Manages the low-level WebSocket connection to the SpacetimeDB server.
 * Handles connecting, disconnecting, sending messages, and receiving messages.
 */
UCLASS(BlueprintType)
class SPACETIMEDBSDK_API UWebsocketManager : public UObject
{
	GENERATED_BODY()

public:
	UWebsocketManager();
	
	virtual void BeginDestroy() override;

	/**
	 * Connects to the WebSocket server at the given URL.
	 * @param ServerUrl The URL of the WebSocket server.
	 */
	void Connect(const FString& ServerUrl);

	/**
	 * Disconnects from the WebSocket server.
	 */
	void Disconnect();

	/**
	 * Sends a message to the WebSocket server.
	 * @param Message The message to send.
	 * @return True if the message was sent successfully, false otherwise.
	 */
	bool SendMessage(const FString& Message);

	/**
	* Sends binary data to the WebSocket server.
	* @param Data The bytes to send.
	* @return True if the message was sent successfully, false otherwise.
	*/
	bool SendMessage(const TArray<uint8>& Data);

	/**
	 * Checks if the WebSocket connection is currently active.
	 * @return True if connected, false otherwise.
	 */
	bool IsConnected() const;

	/**
	* Sets the initial auth token used when connecting.
	* @param Token JWT or session token expected by the server.
	*/
	void SetInitToken(FString Token);

	/** Delegates for WebSocket events */
	UPROPERTY()
	FOnWebSocketConnected OnConnected;

	/** Broadcast when a connection error occurs */
	UPROPERTY()
	FOnWebSocketConnectionError OnConnectionError;

	/** Broadcast for text messages */
	UPROPERTY()
	FOnWebSocketMessageReceived OnMessageReceived;

	/** Broadcast for binary payloads */
	UPROPERTY()
	FOnWebSocketBinaryMessageReceived OnBinaryMessageReceived;

	/** Broadcast when the socket is closed */
	UPROPERTY()
	FOnWebSocketClosed OnClosed;

private:
	/**Underlying WebSocket implementation */
	TSharedPtr<IWebSocket> WebSocket;

	/** Handler for successful connection */
	void HandleConnected();
	/** Handler for connection errors */
	void HandleConnectionError(const FString& Error);
	/** Handler for incoming text messages */
	void HandleMessageReceived(const FString& Message);
	/** Handler for incoming binary messages */
	void HandleBinaryMessageReceived(const void* Data, SIZE_T Size, SIZE_T BytesRemaining);
	/** Handler for socket close */
	void HandleClosed(int32 StatusCode, const FString& Reason, bool bWasClean);

	/** Decompresses a payload based on compression variant */
	bool DecompressPayload(ECompressableQueryUpdateTag Variant, const TArray<uint8>& In, TArray<uint8>& Out);
	/** GZip decompression helper */
	bool DecompressGzip(const TArray<uint8>& InData, TArray<uint8>& OutData);
	/** Brotli decompression helper */
	bool DecompressBrotli(const TArray<uint8>& InData, TArray<uint8>& OutData);

	FString InitToken;

	/** Buffer used to accumulate binary fragments until a complete message
	*  is received. */
	TArray<uint8> IncompleteMessage;

	/** Tracks if we are waiting for additional binary fragments. */
	bool bAwaitingBinaryFragments = false;

};

// Helper function to log a struct as JSON, expanding any transient objects
template <typename StructType>
static void LogAsJson(const StructType& InStruct, const TCHAR* TagName)
{
	FString Json;
	if (!FJsonObjectConverter::UStructToJsonObjectString(InStruct, Json))
	{
		UE_LOG(LogTemp, Warning, TEXT("[%s] Failed to serialize to JSON"), TagName);
		return;
	}

	// Print original JSON
	UE_LOG(LogTemp, Log, TEXT("[%s] %s"), TagName, *Json);

	// Extract object paths like /Script/SpacetimeDbSdk.CompressableQueryUpdateType'/Engine/Transient.CompressableQueryUpdateType_0'
	const FRegexPattern Pattern(TEXT(R"((\/Script\/SpacetimeDbSdk\.\w+)'\/Engine\/Transient\.(\w+))"));
	FRegexMatcher Matcher(Pattern, Json);

	while (Matcher.FindNext())
	{
		FString ClassName = Matcher.GetCaptureGroup(1);  // e.g., /Script/SpacetimeDbSdk.CompressableQueryUpdateType
		FString ObjectName = Matcher.GetCaptureGroup(2);  // e.g., CompressableQueryUpdateType_0

		// Find the object in memory
		UObject* FoundObj = StaticFindObject(UObject::StaticClass(), GetTransientPackage(), *ObjectName);
		if (FoundObj == nullptr)
		{
			UE_LOG(LogTemp, Warning, TEXT("[%s] Could not find object: %s"), TagName, *ObjectName);
			continue;
		}

		// Log its expanded contents
		FString SubJson;
		if (FJsonObjectConverter::UStructToJsonObjectString(
			static_cast<const UStruct*>(FoundObj->GetClass()), FoundObj, SubJson))
		{
			UE_LOG(LogTemp, Log, TEXT("[%s] %s: %s"), TagName, *ObjectName, *SubJson);
		}
		else
		{
			UE_LOG(LogTemp, Warning, TEXT("[%s] Failed to serialize object: %s"), TagName, *ObjectName);
		}
	}
}