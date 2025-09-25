#pragma once

#include "CoreMinimal.h"
#include "HAL/CriticalSection.h"

struct FTestOutcome
{
	bool bSuccess = false;
	FString Error;
};

class FTestCounter : public TSharedFromThis<FTestCounter>
{
public:
	FTestCounter() = default;

	void Register(const FString& TestName);
	void MarkSuccess(const FString& TestName);
	void MarkFailure(const FString& TestName, const FString& Error);

	bool IsComplete() const;
	bool AllSucceeded() const;
	TArray<FString> GetFailures() const;
	TArray<FString> GetSuccesses() const;

	void Abort();
	bool IsAborted() const { return bAborted; }


private:
	mutable FCriticalSection Mutex;
	TMap<FString, FTestOutcome> Outcomes;
	TSet<FString> Registered;
	bool bAborted = false;
};