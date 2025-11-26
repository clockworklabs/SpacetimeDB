#include "Tests/TestCounter.h"

void FTestCounter::Register(const FString& TestName)
{
	FScopeLock Lock(&Mutex);
	if (Registered.Contains(TestName))
	{
		UE_LOG(LogTemp, Error, TEXT("Duplicate test name: %s"), *TestName);
	}
	Registered.Add(TestName);
}

void FTestCounter::MarkSuccess(const FString& TestName)
{
	FScopeLock Lock(&Mutex);
	Outcomes.Add(TestName, { true, FString() });
	UE_LOG(LogTemp, Log, TEXT("Operation success: %s"), *TestName);
}

void FTestCounter::MarkFailure(const FString& TestName, const FString& Error)
{
	FScopeLock Lock(&Mutex);
	Outcomes.Add(TestName, { false, Error });
	UE_LOG(LogTemp, Error, TEXT("Operation failed: %s, %s"), *TestName, *Error);
}

bool FTestCounter::IsComplete() const
{
	FScopeLock Lock(&Mutex);
	return Outcomes.Num() == Registered.Num();
}

bool FTestCounter::AllSucceeded() const
{
	FScopeLock Lock(&Mutex);
	if (Outcomes.Num() != Registered.Num())
	{
		return false;
	}
	for (const auto& Elem : Outcomes)
	{
		if (!Elem.Value.bSuccess)
		{
			return false;
		}
	}
	return true;
}

TArray<FString> FTestCounter::GetFailures() const
{
	FScopeLock Lock(&Mutex);
	TArray<FString> Failures;
	for (const FString& Name : Registered)
	{
		const FTestOutcome* Outcome = Outcomes.Find(Name);
		if (!Outcome)
		{
			Failures.Add(FString::Printf(TEXT("TIMEOUT: %s"), *Name));
		}
		else if (!Outcome->bSuccess)
		{
			Failures.Add(FString::Printf(TEXT("FAILED: %s: %s"), *Name, *Outcome->Error));
		}
	}
	return Failures;
}

TArray<FString> FTestCounter::GetSuccesses() const
{
	FScopeLock Lock(&Mutex);
	TArray<FString> Successes;
	for (const FString& Name : Registered)
	{
		const FTestOutcome* Outcome = Outcomes.Find(Name);
		if (Outcome && Outcome->bSuccess)
		{
			Successes.Add(FString::Printf(TEXT("SUCCESS: %s"), *Name));
		}
	}
	return Successes;
}

void FTestCounter::Abort()
{
	bAborted = true;
}