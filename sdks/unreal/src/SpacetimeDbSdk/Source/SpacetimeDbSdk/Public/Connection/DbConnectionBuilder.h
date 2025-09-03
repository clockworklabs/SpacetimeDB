#pragma once

#include "CoreMinimal.h"
#include "UObject/NoExportTypes.h"
#include "Connection/DbConnectionBase.h"
#include "ModuleBindings/Types/CompressableQueryUpdateType.g.h"
#include "DbConnectionBuilder.generated.h"

UCLASS(BlueprintType)
class SPACETIMEDBSDK_API UDbConnectionBuilderBase : public UObject
{
    GENERATED_BODY()

public:

    /** Set the websocket URI to connect to. */
    UDbConnectionBuilderBase* WithUriBase(const FString& InUri);

    /** Set the remote module/database name. */
    UDbConnectionBuilderBase* WithModuleNameBase(const FString& InName);

    /** Provide an authentication token if available. */
    UDbConnectionBuilderBase* WithTokenBase(const FString& InToken);

    /** Provide an specific compresstion method. Brotli not implemented, will default to Gzip */
    UDbConnectionBuilderBase* WithCompressionBase(const ESpacetimeDBCompression& InCompression);

    //@TODO: Add With Light Mode
    //UDbConnectionBuilderBase* WithLightMode(const bool& bWithLightMode);

    /** Register a callback for successful connect. */
    UDbConnectionBuilderBase* OnConnectBase(FOnConnectBaseDelegate Callback);

    /** Register a callback for connection errors. */
    UDbConnectionBuilderBase* OnConnectErrorBase(FOnConnectErrorDelegate Callback);

    /** Register a callback for disconnect. */
    UDbConnectionBuilderBase* OnDisconnectBase(FOnDisconnectBaseDelegate Callback);

    /** Generic C++ helper for building child types (not visible to Blueprints) */
    UDbConnectionBase* BuildConnection(UDbConnectionBase* Connection);

private:
    FString Uri;
    FString ModuleName;
    FString Token;
    ESpacetimeDBCompression Compression;
    bool bCompressionSet = false;

    FOnConnectErrorDelegate OnConnectErrorCallback;
    FOnConnectBaseDelegate    OnConnectCallback;
    FOnDisconnectBaseDelegate OnDisconnectCallback;
};
