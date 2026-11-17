#include "Connection/Reconnect.h"
#include "Async/Async.h"

void FAutomaticReconnectOptions::Resolve()
{
	MinDelaySeconds = FMath::IsFinite(MinDelaySeconds) ? FMath::Max(0.5f, MinDelaySeconds) : 1.0f;
	MaxDelaySeconds = FMath::IsFinite(MaxDelaySeconds) ? FMath::Max(1.0f, MaxDelaySeconds) : 30.0f;
	MaxDelaySeconds = FMath::Max(MinDelaySeconds, MaxDelaySeconds);
}

double FAutomaticReconnectOptions::Delay(int32 Attempt, double Random) const
{
	const double Base = FMath::Min(double(MaxDelaySeconds),
	                               double(MinDelaySeconds) * FMath::Pow(2.0, FMath::Clamp(Attempt - 1, 0, 30)));
	return FMath::Clamp(Base * (0.5 + Random), double(MinDelaySeconds), double(MaxDelaySeconds));
}

void UReconnectTokenRequest::Complete(const FString& Token, const FString& Error)
{
	if (!IsInGameThread())
	{
		TWeakObjectPtr<UReconnectTokenRequest> WeakThis(this);
		AsyncTask(ENamedThreads::GameThread,
		          [WeakThis, Token, Error]()
		          {
			          if (WeakThis.IsValid())
			          {
				          WeakThis->Complete(Token, Error);
			          }
		          });
		return;
	}
	auto Callback = MoveTemp(Completion);
	Completion = nullptr;
	if (Callback)
	{
		Callback(Token, Error);
	}
}
