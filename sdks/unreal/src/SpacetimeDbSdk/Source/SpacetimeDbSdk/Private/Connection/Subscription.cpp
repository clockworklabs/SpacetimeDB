#include "Connection/Subscription.h"
#include "Connection/DbConnectionBase.h"

USubscriptionHandleBase::USubscriptionHandleBase() {}

void USubscriptionHandleBase::Unsubscribe()
{
	if (bEnded )
	{
		UE_LOG(LogTemp, Warning, TEXT("USubscriptionHandleBase::Unsubscribe called on an already ended handle. Not allowed"));
		return;
	}
	if (bUnsubscribeCalled)
	{
		UE_LOG(LogTemp, Warning, TEXT("USubscriptionHandleBase::Unsubscribe called multiple times for the same handle. Not allowed"));
		return;
	}

	bUnsubscribeCalled = true;

	if (ConnInternal)
	{
		// If we have a connection, we will unsubscribe from it
		ConnInternal->UnsubscribeInternal(this);
	}
	else
	{
		// If we don't have a connection, we just end the subscription
		bEnded = true;
		bActive = false;
		if (EndDelegate.IsBound())
		{
			FSubscriptionEventContextBase Ctx;
			EndDelegate.Execute(Ctx);
		}
	}
}

void USubscriptionHandleBase::UnsubscribeThen(FSubscriptionEventDelegate OnEnd)
{
	// If we have a connection, we will unsubscribe from it and call the end delegate when done
	EndDelegate = OnEnd;
	Unsubscribe();
}

void USubscriptionHandleBase::TriggerApplied(const FSubscriptionEventContextBase& Context)
{
	if (bEnded)
	{
		return;
	}
	bActive = true;
	if (AppliedDelegate.IsBound())
	{
		// If the subscription is active, we execute the applied delegate with the context
		AppliedDelegate.Execute(Context);
	}
}

void USubscriptionHandleBase::TriggerError(const FErrorContextBase& Context)
{
	if (bEnded)
	{
		return;
	}
	bEnded = true;
	bActive = false;
	if (ErrorDelegate.IsBound())
	{
		// If the subscription has an error, we execute the error delegate with the context
		ErrorDelegate.Execute(Context);
	}
}

USubscriptionBuilderBase::USubscriptionBuilderBase() {}

USubscriptionBuilderBase* USubscriptionBuilderBase::OnAppliedBase(FSubscriptionEventDelegate Callback)
{
	AppliedDelegate = Callback;
	return this;
}

USubscriptionBuilderBase* USubscriptionBuilderBase::OnErrorBase(FSubscriptionErrorDelegate Callback)
{
	ErrorDelegate = Callback;
	return this;
}

USubscriptionHandleBase* USubscriptionBuilderBase::SubscribeBase(const TArray<FString>& QuerySqls, USubscriptionHandleBase* Handle)
{
	if (!Handle)
	{
		UE_LOG(LogTemp, Error, TEXT("USubscriptionBuilderBase::SubscribeBase: Handle is null! Returning null handle."));
		return Handle;
	}

	if (QuerySqls.Num() == 0)
	{
		UE_LOG(LogTemp, Warning, TEXT("SubscribeBase called with no query strings"));
	}
	
	Handle->AppliedDelegate = AppliedDelegate;
	Handle->ErrorDelegate = ErrorDelegate;
	Handle->QuerySqls = QuerySqls;
	// Reset delegates so builder can be reused safely
	AppliedDelegate.Unbind();
	ErrorDelegate.Unbind();
	Handle->bActive = false;
	return Handle;
}