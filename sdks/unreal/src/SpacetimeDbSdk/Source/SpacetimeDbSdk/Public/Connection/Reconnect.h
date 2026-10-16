#pragma once

#include "CoreMinimal.h"
#include "UObject/NoExportTypes.h"
#include "Reconnect.generated.h"

/** Unlimited exponential retry with 50% jitter, bounded in seconds. */
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FAutomaticReconnectOptions
{
	GENERATED_BODY()

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB", meta = (ClampMin = "0.5"))
	float MinDelaySeconds = 1.0f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB", meta = (ClampMin = "1.0"))
	float MaxDelaySeconds = 30.0f;

	void Resolve();
	double Delay(int32 Attempt, double Random) const;
};

/** One credential request. Complete it once, possibly after asynchronous authentication.
 * Cancellation makes late completion a no-op. Tokens must belong to the original identity.
 */
UCLASS(BlueprintType)
class SPACETIMEDBSDK_API UReconnectTokenRequest : public UObject
{
	GENERATED_BODY()
public:
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	void Complete(const FString& Token, const FString& Error);

private:
	friend class UDbConnectionBase;
	TFunction<void(const FString&, const FString&)> Completion;
};

DECLARE_DYNAMIC_DELEGATE_OneParam(FTokenProviderDelegate, UReconnectTokenRequest*, Request);
