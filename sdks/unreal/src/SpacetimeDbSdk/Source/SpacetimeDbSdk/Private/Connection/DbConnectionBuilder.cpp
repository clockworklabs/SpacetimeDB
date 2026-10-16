#include "Connection/DbConnectionBuilder.h"
#include "Connection/Websocket.h"
#include "Connection/DbConnectionBase.h"
#include "Connection/LogCategory.h"

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithUriBase(const FString& InUri)
{
	// Check if the URI contains "localhost:" and replace it with adress
	if (InUri.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("WithUriBase called with empty URI, not allowed"));
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

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithDatabaseNameBase(const FString& InName)
{
	if (InName.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning,
		       TEXT("WithDatabaseNameBase called with empty module name, not allowd"));
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
		UE_LOG(LogSpacetimeDb_Connection, Warning,
		       TEXT("Brotli compression is not available in this version of SDK. Defaulting to Gzip."));
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
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("BuildConnection called with null connection object"));
		return nullptr;
	}

	if (Uri.IsEmpty() || ModuleName.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("BuildConnection missing required Uri or ModuleName"));
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

	Connection->bAutomaticReconnect = bAutomaticReconnect;
	Connection->ReconnectOptions = ReconnectOptions;
	Connection->ReconnectOptions.Resolve();
	Connection->TokenProvider = TokenProvider;
	Connection->OnAutomaticReconnectBaseDelegate = OnAutomaticReconnectCallback;
	if (!bCompressionSet)
	{
		Compression = ESpacetimeDBCompression::Gzip;
	}
	const FString CompressionName =
	    StaticEnum<ESpacetimeDBCompression>()->GetNameStringByValue(static_cast<int64>(Compression));
	Connection->WebSocketUrl =
	    FString::Printf(TEXT("%s/v1/database/%s/subscribe?compression=%s"), *WorkUri, *ModuleName, *CompressionName);
	if (bAutomaticReconnect)
	{
		// One replacement session per logical connection, fresh server connection_id per socket.
		Connection->WebSocketUrl += TEXT("&session_id=") + FGuid::NewGuid().ToString(EGuidFormats::Digits);
	}
	Connection->StartSocket();
	return Connection;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithAutomaticReconnectBase()
{
	bAutomaticReconnect = true;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithAutomaticReconnectOptionsBase(
    const FAutomaticReconnectOptions& Options)
{
	ReconnectOptions = Options;
	return WithAutomaticReconnectBase();
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::WithTokenProviderBase(FTokenProviderDelegate Provider)
{
	TokenProvider = Provider;
	return this;
}

UDbConnectionBuilderBase* UDbConnectionBuilderBase::OnAutomaticReconnectBase(FOnConnectBaseDelegate Callback)
{
	OnAutomaticReconnectCallback = Callback;
	return this;
}
