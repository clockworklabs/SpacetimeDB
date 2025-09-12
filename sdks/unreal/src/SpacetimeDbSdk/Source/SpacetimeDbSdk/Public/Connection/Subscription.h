#pragma once

#include "CoreMinimal.h"
#include "UObject/NoExportTypes.h"
#include "Connection/Callback.h"

#include "Subscription.generated.h"

class UDbConnectionBase;

/** Delegate type used for subscription lifecycle events */
DECLARE_DYNAMIC_DELEGATE_OneParam(FSubscriptionEventDelegate, const FSubscriptionEventContextBase&, Context);

/** Delegate type used for subscription error events */
DECLARE_DYNAMIC_DELEGATE_OneParam(FSubscriptionErrorDelegate, const FErrorContextBase&, Context);

/** Handle returned from USubscriptionBuilder::Subscribe */
UCLASS(BlueprintType)
class SPACETIMEDBSDK_API USubscriptionHandleBase : public UObject
{
	GENERATED_BODY()

public:
	USubscriptionHandleBase();

	/** Immediately cancels the subscription */
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	void Unsubscribe();

	/** Cancel the subscription and invoke the provided callback when complete */
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	void UnsubscribeThen(FSubscriptionEventDelegate OnEnd);

	/** True once the subscription has ended */
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	bool IsEnded() const { return bEnded; }

	/** True while the subscription is active */
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	bool IsActive() const { return bActive; }

	/** True if the unsubscibe has been called*/
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	bool IsUnsubscribeCalled() const { return bUnsubscribeCalled; }

	/** Get the SQL queries associated with this subscription */
	UFUNCTION(BlueprintCallable, Category = "SpacetimeDB")
	TArray<FString> GetQuerySqls() const { return QuerySqls; }

	/** Internal API used by the connection to signal events */
	void TriggerApplied(const FSubscriptionEventContextBase& Context);
	/** Internal API used by the connection to signal errors */
	void TriggerError(const FErrorContextBase& Context);

private:

	// Whether the subscription has ended
	bool bEnded = false;
	// Whether the subscription is currently active
	bool bActive = false;
	// Whether the unsubscribe method has been called
	bool bUnsubscribeCalled = false;

	/** Queries associated with this subscription */
	TArray<FString> QuerySqls;

	// Delegate callbacks for subscription events
	FSubscriptionEventDelegate AppliedDelegate;
	FSubscriptionErrorDelegate ErrorDelegate;
	FSubscriptionEventDelegate EndDelegate;

	/** Identifier for this subscription */
	int32 QueryId = -1;

	friend class USubscriptionBuilderBase;
	friend class UDbConnectionBase;
	friend class USubscriptionBuilder;

	/** Owning connection used for subscribe/unsubscribe messages */
	UDbConnectionBase* ConnInternal = nullptr;
};

/** Builder used to construct subscription queries */
UCLASS()
class SPACETIMEDBSDK_API USubscriptionBuilderBase : public UObject
{
	GENERATED_BODY()

public:
	USubscriptionBuilderBase();


protected:

	/** Register a callback to run when the subscription is applied */
	UFUNCTION()
	USubscriptionBuilderBase* OnAppliedBase(FSubscriptionEventDelegate Callback);

	/** Register a callback to run when the subscription fails */
	UFUNCTION()
	USubscriptionBuilderBase* OnErrorBase(FSubscriptionErrorDelegate Callback);

	/** Subscribe to the provided SQL queries */
	UFUNCTION()
	USubscriptionHandleBase* SubscribeBase(const TArray<FString>& QuerySqls, USubscriptionHandleBase* Handle);


private:
	// Delegate callbacks for subscription events
	FSubscriptionEventDelegate AppliedDelegate;
	FSubscriptionErrorDelegate ErrorDelegate;
};
