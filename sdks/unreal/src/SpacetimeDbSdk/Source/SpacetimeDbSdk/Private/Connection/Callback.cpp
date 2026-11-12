#include "Connection/Callback.h"

uint32 UProcedureCallbacks::GetNextRequestId()
{
	return NextRequestIdCounter.fetch_add(1, std::memory_order_relaxed);
}

uint32 UProcedureCallbacks::RegisterCallback(const FOnProcedureCompleteDelegate& Callback)
{
	uint32 RequestId = GetNextRequestId();
	PendingCallbacks.Add(RequestId, Callback);
	return RequestId;
}

bool UProcedureCallbacks::ResolveCallback(uint32 RequestId, const FSpacetimeDBEvent& Event,
                                          const TArray<uint8>& ResultData, bool bSuccess)
{
	if (FOnProcedureCompleteDelegate* Callback = PendingCallbacks.Find(RequestId))
	{
		auto Completion = *Callback;
		PendingCallbacks.Remove(RequestId);
		Completion.ExecuteIfBound(Event, ResultData, bSuccess);
		return true;
	}
	return false;
}

bool UProcedureCallbacks::RemoveCallback(uint32 RequestId)
{
	return PendingCallbacks.Remove(RequestId) > 0;
}

void UProcedureCallbacks::ClearAllCallbacks()
{
	PendingCallbacks.Empty();
}

void UProcedureCallbacks::FailAllCallbacks(const FSpacetimeDBEvent& Event)
{
	auto Callbacks = MoveTemp(PendingCallbacks);
	PendingCallbacks.Empty();
	for (auto& Pair : Callbacks)
	{
		Pair.Value.ExecuteIfBound(Event, TArray<uint8>(), false);
	}
}
