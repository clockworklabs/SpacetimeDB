#include "Connection/DbConnectionBuilder.h"
#include "Connection/Websocket.h"
#include "Connection/DbConnectionBase.h"


UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithUriBase(const FString& InUri)
{
	// Check if the URI contains "localhost:" and replace it with adress
	if (InUri.IsEmpty())
	{
		UE_LOG(LogTemp, Warning, TEXT("WithUriBase called with empty URI, not allowed"));
		return this;
	}
	if (InUri.Contains("localhost:"))
	{
		FString FixedUri = InUri.Replace(TEXT("localhost"), TEXT("127.0.0.1"), ESearchCase::IgnoreCase);
		Uri = FixedUri;
	}
	else
	{
		Uri = InUri;
	}
	return this;
}


UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithModuleNameBase(const FString& InName)
{
	if (InName.IsEmpty())
	{
		UE_LOG(LogTemp, Warning, TEXT("WithModuleNameBase called with empty module name, not allowd"));
	}
	ModuleName = InName;
	return this;
}


UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithTokenBase(const FString& InToken)
{
	Token = InToken;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithCompressionBase(const ESpacetimeDBCompression& InCompression)
{
	if (InCompression == ESpacetimeDBCompression::Brotli)
	{
		UE_LOG(LogTemp, Warning, TEXT("Brotli compression is not available in this version of SDK. Defaulting to Gzip."));
		Compression = ESpacetimeDBCompression::Gzip;
	}
	else
	{
		Compression = InCompression;
	}
	bCompressionSet = true;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::OnConnectBase(FOnConnectBaseDelegate Callback)
{
	OnConnectCallback = Callback;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::OnConnectErrorBase(FOnConnectErrorDelegate Callback)
{
	OnConnectErrorCallback = Callback;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::OnDisconnectBase(FOnDisconnectBaseDelegate Callback)
{
	OnDisconnectCallback = Callback;
	return this;
}

UDbConnectionBase* UDbConnectionBuilderBase::BuildConnection(UDbConnectionBase* Connection)
{

	if (!Connection)
	{
		UE_LOG(LogTemp, Error, TEXT("BuildConnection called with null connection object"));
		return nullptr;
	}

	if (Uri.IsEmpty() || ModuleName.IsEmpty())
	{
		UE_LOG(LogTemp, Error, TEXT("BuildConnection missing required Uri or ModuleName"));
		return nullptr;
	}

	FString WorkUri = Uri;
	WorkUri.TrimStartAndEndInline();

	// Normalize scheme: https->wss, http->ws, default to ws if none provided.
	if (WorkUri.StartsWith(TEXT("https://"), ESearchCase::IgnoreCase))
	{
		WorkUri = TEXT("wss://") + WorkUri.Mid(8);
	}
	else if (WorkUri.StartsWith(TEXT("http://"), ESearchCase::IgnoreCase))
	{
		WorkUri = TEXT("ws://") + WorkUri.Mid(7);
	}
	else if (!WorkUri.StartsWith(TEXT("ws://"), ESearchCase::IgnoreCase) &&
			 !WorkUri.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase))
	{
		WorkUri = TEXT("ws://") + WorkUri;
	}

	if (WorkUri.EndsWith(TEXT("/")))
	{
		WorkUri.LeftChopInline(1);
	}

	Connection->Uri = WorkUri;
	Connection->ModuleName = ModuleName;
	Connection->Token = Token;
	Connection->OnConnectBaseDelegate = OnConnectCallback;
	Connection->OnConnectErrorDelegate = OnConnectErrorCallback;
	Connection->OnDisconnectBaseDelegate = OnDisconnectCallback;

	Connection->WebSocket = NewObject<UWebsocketManager>(Connection);

	//Default to Gzip compression if not set
	if (!bCompressionSet)
	{
		Compression = ESpacetimeDBCompression::Gzip;
	}

	const UEnum* CompressionEnum = StaticEnum<ESpacetimeDBCompression>();
	const FString CompressionName = CompressionEnum->GetNameStringByValue(static_cast<int64>(Compression));

	// Construct the WebSocket URL using the provided URI, module name, and compression type
	FString WebSocketUrl = FString::Printf(TEXT("%s/v1/database/%s/subscribe?compression=%s"),
		*WorkUri,
		*ModuleName,
		*CompressionName);

	Connection->WebSocket->OnConnectionError.AddDynamic(Connection, &UDbConnectionBase::HandleWSError);
	Connection->WebSocket->OnClosed.AddDynamic(Connection, &UDbConnectionBase::HandleWSClosed);
	Connection->WebSocket->OnBinaryMessageReceived.AddDynamic(Connection, &UDbConnectionBase::HandleWSBinaryMessage);
	// Set the initialization token for the WebSocket connection
	Connection->WebSocket->SetInitToken(Token);
	// Connect the WebSocket to the constructed URL
	Connection->WebSocket->Connect(WebSocketUrl);

	return Connection;
}
