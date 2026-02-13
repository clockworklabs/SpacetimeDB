
#pragma once

#include "CoreMinimal.h"
#include "SetReducerFlags.generated.h"


/** Flags controlling reducer call behavior */
UENUM(BlueprintType)
enum class ECallReducerFlags : uint8
{
    /** Default behavior - server will send full update and success notification */
    FullUpdate UMETA(DisplayName = "FullUpdate"),

    /** Do not send success notification after reducer completes */
    NoSuccessNotify UMETA(DisplayName = "NoSuccessNotify"),
};

/** Container for per-reducer call flags */
UCLASS(BlueprintType)
class SPACETIMEDBSDK_API USetReducerFlagsBase : public UObject
{
    GENERATED_BODY()

protected:

    friend class UDbConnectionBase;

    UPROPERTY()
    TMap<FString, ECallReducerFlags> FlagMap;
};