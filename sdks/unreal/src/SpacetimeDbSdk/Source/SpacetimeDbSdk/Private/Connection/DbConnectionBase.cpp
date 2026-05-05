#include "Connection/DbConnectionBase.h"
#include "Connection/DbConnectionBuilder.h"
#include "Connection/Credentials.h"
#include "Connection/LogCategory.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "Misc/Compression.h"
#include "Misc/ScopeLock.h"
#include "HAL/Event.h"
#include "HAL/PlatformProcess.h"
#include "HAL/Runnable.h"
#include "HAL/RunnableThread.h"
#include "ProfilingDebugging/CpuProfilerTrace.h"
#include "BSATN/UEBSATNHelpers.h"
#include "Connection/ProcedureFlags.h"

namespace
{
enum class EWsCompressionTag : uint8
{
	Uncompressed = 0,
	Brotli = 1,
	Gzip = 2,
};

constexpr int32 MaxQueuedInboundRawMessages = 8192;
constexpr int64 MaxQueuedInboundRawBytes = 128ll * 1024ll * 1024ll;
constexpr int32 MaxPendingInboundParsedMessages = 8192;
constexpr int64 MaxInboundDecodedPayloadBytes = 128ll * 1024ll * 1024ll;
constexpr int64 MaxPendingInboundParsedEstimatedBytes = 256ll * 1024ll * 1024ll;
constexpr int32 PendingInboundCompactionMinConsumedMessages = 512;
constexpr int32 InboundRawCompactionMinConsumedMessages = 512;
constexpr uint32 InboundWorkerStackSizeBytes = 0;
constexpr EThreadPriority InboundWorkerThreadPriority = TPri_Normal;
constexpr const TCHAR* InboundWorkerThreadName = TEXT("SpacetimeDBInboundWorker");
constexpr int32 SpacetimeDbCompressionTagBytes = 1;
constexpr int32 GzipFooterUncompressedSizeBytes = 4;
constexpr int32 MaxInboundApplyLogTableContributors = 6;

static FDatabaseUpdateType QueryRowsToDatabaseUpdate(const FQueryRowsType& Rows, UE::SpacetimeDB::EQueryRowsApplyMode Mode)
{
	FDatabaseUpdateType Update;
	for (const FSingleTableRowsType& TableRows : Rows.Tables)
	{
		FTableUpdateType TableUpdate;
		TableUpdate.TableName = TableRows.Table;

		FPersistentTableRowsType PersistentRows;
		switch (Mode)
		{
		case UE::SpacetimeDB::EQueryRowsApplyMode::Deletes:
			PersistentRows.Deletes = TableRows.Rows;
			break;
		case UE::SpacetimeDB::EQueryRowsApplyMode::Inserts:
			PersistentRows.Inserts = TableRows.Rows;
			break;
		default:
			checkf(false, TEXT("Unsupported query-row apply mode for table %s"), *TableRows.Table);
			continue;
		}
		TableUpdate.Rows.Add(FTableUpdateRowsType::PersistentTable(PersistentRows));
		Update.Tables.Add(TableUpdate);
	}
	return Update;
}

static FDatabaseUpdateType TransactionUpdateToDatabaseUpdate(const FTransactionUpdateType& Update)
{
	FDatabaseUpdateType Out;
	for (const FQuerySetUpdateType& QuerySet : Update.QuerySets)
	{
		for (const FTableUpdateType& TableUpdate : QuerySet.Tables)
		{
			Out.Tables.Add(TableUpdate);
		}
	}
	return Out;
}

static FString DecodeReducerErrorMessage(const TArray<uint8>& ErrorBytes)
{
	if (ErrorBytes.Num() == 0)
	{
		return TEXT("Reducer returned empty error payload");
	}
	return UE::SpacetimeDB::Deserialize<FString>(ErrorBytes);
}

static const TCHAR* DescribeServerMessageTag(EServerMessageTag Tag)
{
	switch (Tag)
	{
	case EServerMessageTag::InitialConnection:
		return TEXT("InitialConnection");
	case EServerMessageTag::TransactionUpdate:
		return TEXT("TransactionUpdate");
	case EServerMessageTag::OneOffQueryResult:
		return TEXT("OneOffQueryResult");
	case EServerMessageTag::SubscribeApplied:
		return TEXT("SubscribeApplied");
	case EServerMessageTag::UnsubscribeApplied:
		return TEXT("UnsubscribeApplied");
	case EServerMessageTag::SubscriptionError:
		return TEXT("SubscriptionError");
	case EServerMessageTag::ReducerResult:
		return TEXT("ReducerResult");
	case EServerMessageTag::ProcedureResult:
		return TEXT("ProcedureResult");
	default:
		return TEXT("Unknown");
	}
}

static FString FormatInboundTableApplyStats(const FSpacetimeDBTableApplyStats& Stats)
{
	return FString::Printf(
		TEXT("%s rows=%d ins=%d del=%d bytes=%lld cache=%.2fus broadcast=%.2fus diff=%d"),
		*Stats.TableName,
		Stats.RowSetCount,
		Stats.InsertRowCount,
		Stats.DeleteRowCount,
		Stats.InsertRowBytes + Stats.DeleteRowBytes,
		Stats.CacheMicros,
		Stats.BroadcastMicros,
		Stats.bProducedDiff ? 1 : 0);
}

static int64 EstimatePreprocessedTableDataBytes(const FPreprocessedTableDataMap& PreprocessedTableData)
{
	int64 EstimatedBytes = PreprocessedTableData.GetAllocatedSize();
	for (const TPair<FPreprocessedTableKey, TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>>& TablePair : PreprocessedTableData)
	{
		EstimatedBytes += TablePair.Key.TableName.GetAllocatedSize();
		EstimatedBytes += TablePair.Value.GetAllocatedSize();
		for (const TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>& Data : TablePair.Value)
		{
			if (Data.IsValid())
			{
				EstimatedBytes += Data->EstimateMemoryBytes();
			}
		}
	}
	return EstimatedBytes;
}

static int64 EstimateInboundParsedMessageBytes(const FInboundParsedMessage& Message)
{
	return sizeof(FInboundParsedMessage)
		+ static_cast<int64>(Message.DecodedPayloadSizeBytes)
		+ Message.ProtocolError.GetAllocatedSize()
		+ EstimatePreprocessedTableDataBytes(Message.PreprocessedTableData);
}
}

class FSpacetimeDbInboundWorker final : public FRunnable
{
public:
	explicit FSpacetimeDbInboundWorker(UDbConnectionBase& InConnection)
		: Connection(&InConnection)
	{
		WorkAvailableEvent = FPlatformProcess::GetSynchEventFromPool(false);
		checkf(WorkAvailableEvent != nullptr, TEXT("Failed to allocate SpacetimeDB inbound worker event."));

		Thread = FRunnableThread::Create(
			this,
			InboundWorkerThreadName,
			InboundWorkerStackSizeBytes,
			InboundWorkerThreadPriority);
		checkf(Thread != nullptr, TEXT("Failed to create SpacetimeDB inbound worker thread."));
	}

	virtual ~FSpacetimeDbInboundWorker() override
	{
		StopAndJoin();
	}

	void Notify()
	{
		if (WorkAvailableEvent)
		{
			WorkAvailableEvent->Trigger();
		}
	}

	virtual uint32 Run() override
	{
		while (!bStopRequested)
		{
			checkf(WorkAvailableEvent != nullptr, TEXT("SpacetimeDB inbound worker event was not initialized."));
			WorkAvailableEvent->Wait();

			if (bStopRequested)
			{
				break;
			}

			if (Connection)
			{
				Connection->DrainInboundRawMessagesOnWorker();
			}
		}

		return 0;
	}

	virtual void Stop() override
	{
		bStopRequested = true;
		Notify();
	}

	void StopAndJoin()
	{
		Stop();

		if (Thread)
		{
			Thread->WaitForCompletion();
			delete Thread;
			Thread = nullptr;
		}

		if (WorkAvailableEvent)
		{
			FPlatformProcess::ReturnSynchEventToPool(WorkAvailableEvent);
			WorkAvailableEvent = nullptr;
		}

		Connection = nullptr;
	}

private:
	UDbConnectionBase* Connection = nullptr;
	FEvent* WorkAvailableEvent = nullptr;
	FRunnableThread* Thread = nullptr;
	FThreadSafeBool bStopRequested = false;
};

UDbConnectionBase::UDbConnectionBase(const FObjectInitializer& ObjectInitializer)
	: Super(ObjectInitializer)
{
	NextRequestId = 1;
	NextSubscriptionId = 1;
	ProcedureCallbacks = CreateDefaultSubobject<UProcedureCallbacks>(TEXT("ProcedureCallbacks"));
}

void UDbConnectionBase::BeginDestroy()
{
	StopInboundMessageWorker();
	Super::BeginDestroy();
}

void UDbConnectionBase::Disconnect()
{
	StopInboundMessageWorker();
	if (WebSocket)
	{
		WebSocket->Disconnect();
	}
}

bool UDbConnectionBase::IsActive() const
{
	return WebSocket && WebSocket->IsConnected();
}


bool UDbConnectionBase::TryGetIdentity(FSpacetimeDBIdentity& OutIdentity) const
{
	if (bIsIdentitySet)
	{
		OutIdentity = Identity;
		return true;
	}

	UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("TryGetIdentity called before identity was set"));
	return false;
}

FSpacetimeDBConnectionId UDbConnectionBase::GetConnectionId() const
{
	return ConnectionId;
}

bool UDbConnectionBase::SendRawMessage(const FString& Message)
{
	return WebSocket && WebSocket->SendMessage(Message);
}

bool UDbConnectionBase::SendRawMessage(const TArray<uint8>& Message)
{
	return WebSocket && WebSocket->SendMessage(Message);
}

USubscriptionBuilderBase* UDbConnectionBase::SubscriptionBuilderBase()
{
	return NewObject<USubscriptionBuilderBase>();
}

void UDbConnectionBase::HandleWSError(const FString& Error)
{
	StopInboundMessageWorker();
	bProtocolViolationHandled = false;
	ClearPendingOperations(Error);
	if (OnConnectErrorDelegate.IsBound())
	{
		OnConnectErrorDelegate.Execute(Error);
	}
}

void UDbConnectionBase::HandleWSClosed(int32 /*StatusCode*/, const FString& Reason, bool /*bWasClean*/)
{
	StopInboundMessageWorker();
	bProtocolViolationHandled = false;
	ClearPendingOperations(Reason);
	if (OnDisconnectBaseDelegate.IsBound())
	{
		OnDisconnectBaseDelegate.Execute(this, Reason);
	}
}

void UDbConnectionBase::HandleProtocolViolation(const FString& ErrorMessage)
{
	if (bProtocolViolationHandled)
	{
		return;
	}
	bProtocolViolationHandled = true;

	UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("%s"), *ErrorMessage);
	TriggerError(ErrorMessage);
	StopInboundMessageWorker();
	ClearPendingOperations(ErrorMessage);

	// Match Rust/C# behavior: parse/protocol violations are fatal for the connection.
	if (WebSocket && WebSocket->IsConnected())
	{
		WebSocket->Disconnect();
	}
	else if (OnConnectErrorDelegate.IsBound())
	{
		OnConnectErrorDelegate.Execute(ErrorMessage);
	}
}

void UDbConnectionBase::StartInboundMessageWorker()
{
	FScopeLock Lock(&InboundWorkerMutex);
	if (InboundWorker)
	{
		return;
	}

	{
		FScopeLock RawLock(&InboundRawMessagesMutex);
		InboundRawMessages.Reset();
		InboundRawMessageReadIndex = 0;
		InboundQueuedRawBytes = 0;
		++InboundConnectionEpoch;
		NextInboundSequenceId = 0;
		bInboundAcceptingMessages = true;
		bInboundProtocolErrorQueued = false;
	}

	{
		FScopeLock PendingLock(&PendingMessagesMutex);
		PendingMessages.Reset();
		PendingMessageReadIndex = 0;
		PendingParsedEstimatedBytes = 0;
	}

	ActivePreprocessedTableData = nullptr;
	InboundWorker = new FSpacetimeDbInboundWorker(*this);
}

void UDbConnectionBase::StopInboundMessageWorker()
{
	FSpacetimeDbInboundWorker* WorkerToStop = nullptr;
	{
		FScopeLock Lock(&InboundWorkerMutex);
		{
			FScopeLock RawLock(&InboundRawMessagesMutex);
			InboundRawMessages.Reset();
			InboundRawMessageReadIndex = 0;
			InboundQueuedRawBytes = 0;
			++InboundConnectionEpoch;
			bInboundAcceptingMessages = false;
			bInboundProtocolErrorQueued = false;
		}

		{
			FScopeLock PendingLock(&PendingMessagesMutex);
			PendingMessages.Reset();
			PendingMessageReadIndex = 0;
			PendingParsedEstimatedBytes = 0;
		}

		ActivePreprocessedTableData = nullptr;
		WorkerToStop = InboundWorker;
		InboundWorker = nullptr;
	}

	if (WorkerToStop)
	{
		delete WorkerToStop;
	}

	ClearInboundMessageQueues();
}

void UDbConnectionBase::ClearInboundMessageQueues()
{
	{
		FScopeLock Lock(&InboundRawMessagesMutex);
		InboundRawMessages.Reset();
		InboundRawMessageReadIndex = 0;
		InboundQueuedRawBytes = 0;
	}

	{
		FScopeLock Lock(&PendingMessagesMutex);
		PendingMessages.Reset();
		PendingMessageReadIndex = 0;
		PendingParsedEstimatedBytes = 0;
	}

	ActivePreprocessedTableData = nullptr;
}

void UDbConnectionBase::NotifyInboundWorkerIfNeeded()
{
	FScopeLock WorkerLock(&InboundWorkerMutex);
	if (InboundWorker == nullptr)
	{
		return;
	}

	bool bShouldNotify = false;
	{
		FScopeLock RawLock(&InboundRawMessagesMutex);
		checkf(InboundRawMessageReadIndex <= InboundRawMessages.Num(),
			TEXT("SpacetimeDB inbound raw queue read index exceeded queued messages while notifying worker."));
		bShouldNotify = InboundRawMessages.Num() > InboundRawMessageReadIndex && bInboundAcceptingMessages && !bInboundProtocolErrorQueued;
	}

	if (bShouldNotify)
	{
		InboundWorker->Notify();
	}
}

bool UDbConnectionBase::IsInboundProtocolErrorQueued() const
{
	FScopeLock Lock(&InboundRawMessagesMutex);
	return bInboundProtocolErrorQueued;
}

bool UDbConnectionBase::IsInboundEpochCurrentAndAccepting(uint64 ConnectionEpoch) const
{
	FScopeLock Lock(&InboundRawMessagesMutex);
	return bInboundAcceptingMessages && !bInboundProtocolErrorQueued && InboundConnectionEpoch == ConnectionEpoch;
}

void UDbConnectionBase::MarkInboundProtocolErrorQueued()
{
	FScopeLock Lock(&InboundRawMessagesMutex);
	bInboundProtocolErrorQueued = true;
	bInboundAcceptingMessages = false;
	InboundRawMessages.Reset();
	InboundRawMessageReadIndex = 0;
	InboundQueuedRawBytes = 0;
}

void UDbConnectionBase::EnqueueInboundProtocolError(uint64 SequenceId, int32 PayloadSizeBytes, uint8 CompressionTag, int32 QueueDepthAtEnqueue, int64 QueuedBytesAtEnqueue, const FString& ErrorMessage)
{
	UE_LOG(
		LogSpacetimeDb_Connection,
		Error,
		TEXT("SpacetimeDB inbound protocol error: sequence=%llu payload_bytes=%d compression_tag=%u queued_messages=%d queued_bytes=%lld detail=%s"),
		SequenceId,
		PayloadSizeBytes,
		static_cast<uint32>(CompressionTag),
		QueueDepthAtEnqueue,
		QueuedBytesAtEnqueue,
		*ErrorMessage);

	FInboundParsedMessage Parsed;
	Parsed.SequenceId = SequenceId;
	Parsed.PayloadSizeBytes = PayloadSizeBytes;
	Parsed.CompressionTag = CompressionTag;
	Parsed.QueueDepthAtEnqueue = QueueDepthAtEnqueue;
	Parsed.QueuedBytesAtEnqueue = QueuedBytesAtEnqueue;
	Parsed.bProtocolError = true;
	Parsed.ProtocolError = ErrorMessage;
	Parsed.EstimatedMemoryBytes = EstimateInboundParsedMessageBytes(Parsed);

	FScopeLock Lock(&PendingMessagesMutex);
	PendingMessages.Reset();
	PendingMessageReadIndex = 0;
	PendingParsedEstimatedBytes = 0;
	PendingMessages.Add(MoveTemp(Parsed));
	PendingParsedEstimatedBytes += PendingMessages[0].EstimatedMemoryBytes;
}

void UDbConnectionBase::HandleWSBinaryMessage(const TArray<uint8>& Message)
{
	TArray<uint8> OwnedMessage = Message;
	HandleWSBinaryMessageOwned(MoveTemp(OwnedMessage));
}

void UDbConnectionBase::HandleWSBinaryMessageOwned(TArray<uint8>&& Message)
{
	TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_InboundEnqueue);

	const int32 PayloadSizeBytes = Message.Num();
	const uint8 CompressionTag = PayloadSizeBytes > 0 ? Message[0] : 0;
	uint64 SequenceId = 0;
	int32 QueueDepthAtEnqueue = 0;
	int64 QueuedBytesAtEnqueue = 0;
	uint64 ConnectionEpoch = 0;
	bool bQueueOverloaded = false;
	FString QueueOverloadError;

	{
		FScopeLock WorkerLock(&InboundWorkerMutex);
		FScopeLock RawLock(&InboundRawMessagesMutex);
		if (!bInboundAcceptingMessages || bInboundProtocolErrorQueued)
		{
			return;
		}
		checkf(InboundWorker != nullptr, TEXT("SpacetimeDB inbound worker missing while inbound connection epoch %llu is accepting messages."), InboundConnectionEpoch);

		ConnectionEpoch = InboundConnectionEpoch;
		SequenceId = NextInboundSequenceId++;
		checkf(InboundRawMessageReadIndex <= InboundRawMessages.Num(),
			TEXT("SpacetimeDB inbound raw queue read index exceeded queued messages while enqueuing payload."));
		const int64 NewQueuedRawBytes = InboundQueuedRawBytes + static_cast<int64>(PayloadSizeBytes);
		const int32 LiveQueuedRawMessageCount = InboundRawMessages.Num() - InboundRawMessageReadIndex;
		const int32 NewQueuedRawMessageCount = LiveQueuedRawMessageCount + 1;
		QueueDepthAtEnqueue = NewQueuedRawMessageCount;
		QueuedBytesAtEnqueue = NewQueuedRawBytes;
		if (NewQueuedRawMessageCount > MaxQueuedInboundRawMessages || NewQueuedRawBytes > MaxQueuedInboundRawBytes)
		{
			bInboundProtocolErrorQueued = true;
			bInboundAcceptingMessages = false;
			InboundRawMessages.Reset();
			InboundRawMessageReadIndex = 0;
			InboundQueuedRawBytes = 0;
			bQueueOverloaded = true;
			QueueOverloadError = FString::Printf(
				TEXT("SpacetimeDB inbound queue overload: sequence=%llu payload_bytes=%d compression_tag=%u queued_messages=%d queued_bytes=%lld max_messages=%d max_bytes=%lld"),
				SequenceId,
				PayloadSizeBytes,
				static_cast<uint32>(CompressionTag),
				NewQueuedRawMessageCount,
				NewQueuedRawBytes,
				MaxQueuedInboundRawMessages,
				MaxQueuedInboundRawBytes);
		}
		else
		{
			FInboundRawMessage RawMessage;
			RawMessage.ConnectionEpoch = ConnectionEpoch;
			RawMessage.SequenceId = SequenceId;
			RawMessage.QueueDepthAtEnqueue = QueueDepthAtEnqueue;
			RawMessage.QueuedBytesAtEnqueue = QueuedBytesAtEnqueue;
			RawMessage.Payload = MoveTemp(Message);
			InboundRawMessages.Add(MoveTemp(RawMessage));
			InboundQueuedRawBytes = NewQueuedRawBytes;
			InboundWorker->Notify();
		}
	}

	if (bQueueOverloaded)
	{
		EnqueueInboundProtocolError(SequenceId, PayloadSizeBytes, CompressionTag, QueueDepthAtEnqueue, QueuedBytesAtEnqueue, QueueOverloadError);
		return;
	}
}

void UDbConnectionBase::FrameTick()
{
	int32 MessagesProcessed = 0;
	int64 PayloadBytesProcessed = 0;
	const uint64 FrameStartCycles = FPlatformTime::Cycles64();
	const bool bDrainAllPendingMessages = InboundApplyBudget.bDrainAllPendingMessages;
	{
		FScopeLock Lock(&PendingMessagesMutex);
		const int32 PendingCount = PendingMessages.Num() - PendingMessageReadIndex;
		if (PendingCount <= 0)
		{
			//nothing to process, return early
			return;
		}

	}

	TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_GameThreadApplyInbound);

	while (true)
	{
		FInboundParsedMessage Msg;
		{
			FScopeLock Lock(&PendingMessagesMutex);
			if (PendingMessageReadIndex >= PendingMessages.Num())
			{
				PendingMessages.Reset();
				PendingMessageReadIndex = 0;
				PendingParsedEstimatedBytes = 0;
				break;
			}

			if (!bDrainAllPendingMessages && MessagesProcessed >= InboundApplyBudget.MaxMessagesPerFrame)
			{
				break;
			}

			FInboundParsedMessage& PendingMessage = PendingMessages[PendingMessageReadIndex];
			const int64 PendingPayloadBytes = static_cast<int64>(PendingMessage.PayloadSizeBytes);
			const int64 PendingEstimatedBytes = PendingMessage.EstimatedMemoryBytes;
			if (!bDrainAllPendingMessages && MessagesProcessed > 0 && PayloadBytesProcessed + PendingPayloadBytes > InboundApplyBudget.MaxPayloadBytesPerFrame)
			{
				break;
			}

			PayloadBytesProcessed += PendingPayloadBytes;
			PendingParsedEstimatedBytes = FMath::Max<int64>(0, PendingParsedEstimatedBytes - PendingEstimatedBytes);
			Msg = MoveTemp(PendingMessage);
			++PendingMessageReadIndex;

			if (PendingMessageReadIndex == PendingMessages.Num())
			{
				PendingMessages.Reset();
				PendingMessageReadIndex = 0;
				PendingParsedEstimatedBytes = 0;
			}
			else if (PendingMessageReadIndex >= PendingInboundCompactionMinConsumedMessages)
			{
				PendingMessages.RemoveAt(0, PendingMessageReadIndex, EAllowShrinking::No);
				PendingMessageReadIndex = 0;
			}
		}

		if (Msg.bProtocolError)
		{
			HandleProtocolViolation(Msg.ProtocolError);
			break;
		}

		const uint64 MessageStartCycles = FPlatformTime::Cycles64();
		FSpacetimeDBInboundMessageApplyStats ApplyStats;
		ApplyStats.MessageKind = DescribeServerMessageTag(Msg.Message.Tag);
		ApplyStats.SequenceId = Msg.SequenceId;
		ApplyStats.PayloadSizeBytes = Msg.PayloadSizeBytes;
		ApplyStats.QueueDepthAtEnqueue = Msg.QueueDepthAtEnqueue;
		ApplyStats.QueuedBytesAtEnqueue = Msg.QueuedBytesAtEnqueue;
		if (Msg.Message.Tag == EServerMessageTag::ReducerResult)
		{
			const FReducerResultType& Payload = Msg.Message.MessageData.Get<FReducerResultType>();
			ApplyStats.RequestId = Payload.RequestId;
			if (const FReducerCallInfoType* FoundReducerCall = PendingReducerCalls.Find(Payload.RequestId))
			{
				ApplyStats.ReducerName = FoundReducerCall->ReducerName;
			}
		}

		ProcessInboundServerMessage(Msg, ApplyStats);
		const double MessageElapsedMicros =
			FPlatformTime::ToMilliseconds64(FPlatformTime::Cycles64() - MessageStartCycles) * 1000.0;
		if (!bDrainAllPendingMessages &&
			InboundApplyBudget.SoftTimeBudgetMicros > 0 &&
			MessageElapsedMicros >= static_cast<double>(InboundApplyBudget.SoftTimeBudgetMicros))
		{
			TArray<FSpacetimeDBTableApplyStats> SortedStats = ApplyStats.TableStats;
			SortedStats.Sort([](const FSpacetimeDBTableApplyStats& A, const FSpacetimeDBTableApplyStats& B)
			{
				return (A.CacheMicros + A.BroadcastMicros) > (B.CacheMicros + B.BroadcastMicros);
			});
			TArray<FString> TopTableSummaries;
			const int32 LoggedTableCount = FMath::Min(SortedStats.Num(), MaxInboundApplyLogTableContributors);
			TopTableSummaries.Reserve(LoggedTableCount);
			for (int32 TableIndex = 0; TableIndex < LoggedTableCount; ++TableIndex)
			{
				TopTableSummaries.Add(FormatInboundTableApplyStats(SortedStats[TableIndex]));
			}
			const FString TopTablesText = TopTableSummaries.IsEmpty()
				? TEXT("<none>")
				: FString::Join(TopTableSummaries, TEXT(" | "));
			UE_LOG(LogSpacetimeDb_Connection,
				Warning,
				TEXT("SpacetimeDB inbound single-message apply exceeded soft budget: %.2fus >= %lldus kind=%s sequence=%llu request_id=%u reducer=%s payload_bytes=%d queued_messages=%d queued_bytes=%lld messages_processed_before=%d tables=%d top_tables=%s"),
				MessageElapsedMicros,
				InboundApplyBudget.SoftTimeBudgetMicros,
				*ApplyStats.MessageKind,
				ApplyStats.SequenceId,
				ApplyStats.RequestId,
				ApplyStats.ReducerName.IsEmpty() ? TEXT("<none>") : *ApplyStats.ReducerName,
				ApplyStats.PayloadSizeBytes,
				ApplyStats.QueueDepthAtEnqueue,
				ApplyStats.QueuedBytesAtEnqueue,
				MessagesProcessed,
				ApplyStats.TableStats.Num(),
				*TopTablesText);
		}
		++MessagesProcessed;

		if (!bDrainAllPendingMessages &&
			MessagesProcessed >= InboundApplyBudget.MinMessagesPerFrame &&
			InboundApplyBudget.SoftTimeBudgetMicros > 0)
		{
			const double ElapsedMicros = FPlatformTime::ToMilliseconds64(FPlatformTime::Cycles64() - FrameStartCycles) * 1000.0;
			if (ElapsedMicros >= static_cast<double>(InboundApplyBudget.SoftTimeBudgetMicros))
			{
				break;
			}
		}
	}

	if (MessagesProcessed > 0)
	{
		NotifyInboundWorkerIfNeeded();
	}
}

void UDbConnectionBase::DrainInboundRawMessagesOnWorker()
{
	TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_InboundWorkerDrain);

	while (!IsInboundProtocolErrorQueued())
	{
		int32 ParsedMessageCapacity = 0;
		int64 ParsedEstimatedByteCapacity = 0;
		{
			FScopeLock Lock(&PendingMessagesMutex);
			const int32 LivePendingMessages = PendingMessages.Num() - PendingMessageReadIndex;
			ParsedMessageCapacity = MaxPendingInboundParsedMessages - LivePendingMessages;
			ParsedEstimatedByteCapacity = MaxPendingInboundParsedEstimatedBytes - PendingParsedEstimatedBytes;
		}

		if (ParsedMessageCapacity <= 0 || ParsedEstimatedByteCapacity <= 0)
		{
			return;
		}

		TArray<FInboundRawMessage> LocalRawMessages;
		int64 DrainedRawBytes = 0;
		{
			FScopeLock Lock(&InboundRawMessagesMutex);
			checkf(InboundRawMessageReadIndex <= InboundRawMessages.Num(),
				TEXT("SpacetimeDB inbound raw queue read index exceeded queued messages while draining worker queue."));
			const int32 LiveRawMessageCount = InboundRawMessages.Num() - InboundRawMessageReadIndex;
			if (LiveRawMessageCount <= 0 || !bInboundAcceptingMessages || bInboundProtocolErrorQueued)
			{
				return;
			}

			int32 DrainCount = 0;
			for (; DrainCount < LiveRawMessageCount && DrainCount < ParsedMessageCapacity; ++DrainCount)
			{
				const FInboundRawMessage& Candidate = InboundRawMessages[InboundRawMessageReadIndex + DrainCount];
				const int64 NextPayloadBytes = static_cast<int64>(Candidate.Payload.Num());
				if (DrainCount > 0 && DrainedRawBytes + NextPayloadBytes > ParsedEstimatedByteCapacity)
				{
					break;
				}
				if (DrainCount == 0 && NextPayloadBytes > ParsedEstimatedByteCapacity)
				{
					return;
				}

				DrainedRawBytes += NextPayloadBytes;
			}

			if (DrainCount == 0)
			{
				return;
			}

			LocalRawMessages.Reserve(DrainCount);
			for (int32 Index = 0; Index < DrainCount; ++Index)
			{
				LocalRawMessages.Add(MoveTemp(InboundRawMessages[InboundRawMessageReadIndex + Index]));
			}

			InboundRawMessageReadIndex += DrainCount;
			if (InboundRawMessageReadIndex == InboundRawMessages.Num())
			{
				InboundRawMessages.Reset();
				InboundRawMessageReadIndex = 0;
			}
			else if (InboundRawMessageReadIndex >= InboundRawCompactionMinConsumedMessages)
			{
				InboundRawMessages.RemoveAt(0, InboundRawMessageReadIndex, EAllowShrinking::No);
				InboundRawMessageReadIndex = 0;
			}
			InboundQueuedRawBytes = FMath::Max<int64>(0, InboundQueuedRawBytes - DrainedRawBytes);
		}

		TArray<FInboundParsedMessage> LocalParsedMessages;
		LocalParsedMessages.Reserve(LocalRawMessages.Num());

		for (FInboundRawMessage& RawMessage : LocalRawMessages)
		{
			if (!IsInboundEpochCurrentAndAccepting(RawMessage.ConnectionEpoch))
			{
				return;
			}

			FInboundParsedMessage ParsedMessage;
			if (!BuildInboundParsedMessage(RawMessage, ParsedMessage))
			{
				if (!IsInboundEpochCurrentAndAccepting(RawMessage.ConnectionEpoch))
				{
					return;
				}
				MarkInboundProtocolErrorQueued();
				LocalParsedMessages.Add(MoveTemp(ParsedMessage));
				break;
			}

			if (!IsInboundEpochCurrentAndAccepting(RawMessage.ConnectionEpoch))
			{
				return;
			}
			LocalParsedMessages.Add(MoveTemp(ParsedMessage));
		}

		if (LocalParsedMessages.Num() == 0)
		{
			continue;
		}

		const bool bBatchEndsWithProtocolError = LocalParsedMessages.Last().bProtocolError;
		if (IsInboundProtocolErrorQueued() && !bBatchEndsWithProtocolError)
		{
			return;
		}
		if (!bBatchEndsWithProtocolError && !IsInboundEpochCurrentAndAccepting(LocalParsedMessages[0].ConnectionEpoch))
		{
			return;
		}

		uint64 OverloadSequenceId = LocalParsedMessages[0].SequenceId;
		int32 OverloadPayloadSizeBytes = LocalParsedMessages[0].PayloadSizeBytes;
		uint8 OverloadCompressionTag = LocalParsedMessages[0].CompressionTag;
		bool bParsedQueueOverloaded = false;
		FString ParsedQueueOverloadError;
		{
			FScopeLock Lock(&PendingMessagesMutex);
			int64 AddedEstimatedBytes = 0;
			for (const FInboundParsedMessage& ParsedMessage : LocalParsedMessages)
			{
				AddedEstimatedBytes += ParsedMessage.EstimatedMemoryBytes;
			}

			const int32 LivePendingMessages = PendingMessages.Num() - PendingMessageReadIndex;
			const int32 NewPendingMessageCount = LivePendingMessages + LocalParsedMessages.Num();
			const int64 NewPendingEstimatedBytes = PendingParsedEstimatedBytes + AddedEstimatedBytes;
			bParsedQueueOverloaded = !bBatchEndsWithProtocolError &&
				(NewPendingMessageCount > MaxPendingInboundParsedMessages ||
					NewPendingEstimatedBytes > MaxPendingInboundParsedEstimatedBytes);
			if (bParsedQueueOverloaded)
			{
				ParsedQueueOverloadError = FString::Printf(
					TEXT("SpacetimeDB parsed inbound queue overload: sequence=%llu payload_bytes=%d compression_tag=%u queued_messages=%d estimated_bytes=%lld max_messages=%d max_estimated_bytes=%lld"),
					OverloadSequenceId,
					OverloadPayloadSizeBytes,
					static_cast<uint32>(OverloadCompressionTag),
					NewPendingMessageCount,
					NewPendingEstimatedBytes,
					MaxPendingInboundParsedMessages,
					MaxPendingInboundParsedEstimatedBytes);
			}
			else
			{
				PendingMessages.Append(MoveTemp(LocalParsedMessages));
				PendingParsedEstimatedBytes = NewPendingEstimatedBytes;
			}
		}

		if (bParsedQueueOverloaded)
		{
			MarkInboundProtocolErrorQueued();
			EnqueueInboundProtocolError(
				OverloadSequenceId,
				OverloadPayloadSizeBytes,
				OverloadCompressionTag,
				LocalParsedMessages[0].QueueDepthAtEnqueue,
				LocalParsedMessages[0].QueuedBytesAtEnqueue,
				ParsedQueueOverloadError);
			return;
		}
	}
}

bool UDbConnectionBase::BuildInboundParsedMessage(const FInboundRawMessage& RawMessage, FInboundParsedMessage& OutMessage)
{
	TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_InboundPreprocess);

	OutMessage.ConnectionEpoch = RawMessage.ConnectionEpoch;
	OutMessage.SequenceId = RawMessage.SequenceId;
	OutMessage.PayloadSizeBytes = RawMessage.Payload.Num();
	OutMessage.CompressionTag = RawMessage.Payload.Num() > 0 ? RawMessage.Payload[0] : 0;
	OutMessage.QueueDepthAtEnqueue = RawMessage.QueueDepthAtEnqueue;
	OutMessage.QueuedBytesAtEnqueue = RawMessage.QueuedBytesAtEnqueue;

	if (!PreProcessMessage(RawMessage.Payload, OutMessage))
	{
		OutMessage.bProtocolError = true;
		OutMessage.ProtocolError = FString::Printf(
			TEXT("Failed to parse/decompress incoming WebSocket message: sequence=%llu payload_bytes=%d compression_tag=%u queued_messages=%d queued_bytes=%lld"),
			OutMessage.SequenceId,
			OutMessage.PayloadSizeBytes,
			static_cast<uint32>(OutMessage.CompressionTag),
			OutMessage.QueueDepthAtEnqueue,
			OutMessage.QueuedBytesAtEnqueue);
		OutMessage.EstimatedMemoryBytes = EstimateInboundParsedMessageBytes(OutMessage);
		return false;
	}

	return true;
}

void UDbConnectionBase::Tick(float DeltaTime)
{
	if (bIsAutoTicking)
	{
		FrameTick();
	}
}

TStatId UDbConnectionBase::GetStatId() const
{
	// This is used by the engine to track tickables, we return a unique stat ID for this class
	RETURN_QUICK_DECLARE_CYCLE_STAT(UMyTickableObject, STATGROUP_Tickables);
}

bool UDbConnectionBase::IsTickable() const
{
	return bIsAutoTicking;
}

bool UDbConnectionBase::IsTickableInEditor() const
{
	return bIsAutoTicking;
}


void UDbConnectionBase::ProcessInboundServerMessage(FInboundParsedMessage& InboundMessage, FSpacetimeDBInboundMessageApplyStats& ApplyStats)
{
	struct FActivePreprocessedDataGuard
	{
		FPreprocessedTableDataMap*& Target;
		FSpacetimeDBInboundMessageApplyStats*& StatsTarget;

		FActivePreprocessedDataGuard(
			FPreprocessedTableDataMap*& InTarget,
			FPreprocessedTableDataMap* InValue,
			FSpacetimeDBInboundMessageApplyStats*& InStatsTarget,
			FSpacetimeDBInboundMessageApplyStats* InStatsValue)
			: Target(InTarget)
			, StatsTarget(InStatsTarget)
		{
			checkf(Target == nullptr, TEXT("Nested SpacetimeDB inbound table preprocessing scope detected."));
			checkf(StatsTarget == nullptr, TEXT("Nested SpacetimeDB inbound apply stats scope detected."));
			Target = InValue;
			StatsTarget = InStatsValue;
		}

		~FActivePreprocessedDataGuard()
		{
			Target = nullptr;
			StatsTarget = nullptr;
		}
	};

	FActivePreprocessedDataGuard Guard(
		ActivePreprocessedTableData,
		&InboundMessage.PreprocessedTableData,
		ActiveInboundMessageApplyStats,
		&ApplyStats);
	ProcessServerMessage(InboundMessage.Message);
}

void UDbConnectionBase::ProcessServerMessage(const FServerMessageType& Message)
{
	switch (Message.Tag)
	{
	case EServerMessageTag::InitialConnection:
	{
		const FInitialConnectionType& Payload = Message.MessageData.Get<FInitialConnectionType>();
		Token = Payload.Token;
		UCredentials::SaveToken(Token);
		Identity = Payload.Identity;
		bIsIdentitySet = true;
		ConnectionId = Payload.ConnectionId;
		if (OnConnectBaseDelegate.IsBound())
		{
			OnConnectBaseDelegate.Execute(this, Identity, Token);
		}
		break;
	}
	case EServerMessageTag::TransactionUpdate:
	{
		const FTransactionUpdateType& Payload = Message.MessageData.Get<FTransactionUpdateType>();
		const FDatabaseUpdateType Update = TransactionUpdateToDatabaseUpdate(Payload);
		DbUpdate(Update, FSpacetimeDBEvent::Transaction(FSpacetimeDBUnit()));
		break;
	}
	case EServerMessageTag::OneOffQueryResult:
	{
		// One-off query results are request/response only and do not mutate cache by default.
		break;
	}
	case EServerMessageTag::SubscribeApplied:
	{
		const FSubscribeAppliedType& Payload = Message.MessageData.Get<FSubscribeAppliedType>();
		const FDatabaseUpdateType Update = QueryRowsToDatabaseUpdate(Payload.Rows, UE::SpacetimeDB::EQueryRowsApplyMode::Inserts);
		DbUpdate(Update, FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit()));

		if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
		{
			TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
			if (!Handle)
			{
				UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("SubscribeApplied: Null handle for QuerySetId %u."), Payload.QuerySetId.Id);
				return;
			}
			FSubscriptionEventContextBase Ctx;
			Ctx.Event = FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit());
			Handle->TriggerApplied(Ctx);
		}
		break;
	}
	case EServerMessageTag::UnsubscribeApplied:
	{
		const FUnsubscribeAppliedType& Payload = Message.MessageData.Get<FUnsubscribeAppliedType>();
		if (Payload.Rows.IsSet())
		{
			const FDatabaseUpdateType Update = QueryRowsToDatabaseUpdate(Payload.Rows.Value, UE::SpacetimeDB::EQueryRowsApplyMode::Deletes);
			DbUpdate(Update, FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit()));
		}

		if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
		{
			TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
			if (!Handle)
			{
				UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UnsubscribeApplied: Null handle for QuerySetId %u."), Payload.QuerySetId.Id);
				return;
			}
			Handle->bEnded = true;
			Handle->bActive = false;
			Handle->bUnsubscribeCalled = true;
			FSubscriptionEventContextBase Ctx;
			Ctx.Event = FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit());
			if (Handle->EndDelegate.IsBound())
			{
				Handle->EndDelegate.Execute(Ctx);
			}
			ActiveSubscriptions.Remove(Payload.QuerySetId.Id);
		}
		break;
	}
	case EServerMessageTag::SubscriptionError:
	{
		const FSubscriptionErrorType& Payload = Message.MessageData.Get<FSubscriptionErrorType>();
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("SubscriptionError received for QuerySetId=%u Error=%s"),
			Payload.QuerySetId.Id,
			*Payload.Error);
		if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
		{
			TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
			if (!Handle)
			{
				UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("SubscriptionError: Null handle for QuerySetId %u. Error: %s"),
					Payload.QuerySetId.Id,
					*Payload.Error);
				return;
			}
			FErrorContextBase Ctx; Ctx.Error = Payload.Error;
			Handle->TriggerError(Ctx);
			ActiveSubscriptions.Remove(Payload.QuerySetId.Id);
		}
		break;
	}
	case EServerMessageTag::ReducerResult:
	{
		const FReducerResultType& Payload = Message.MessageData.Get<FReducerResultType>();
		const FReducerCallInfoType* FoundReducerCall = PendingReducerCalls.Find(Payload.RequestId);
		if (!FoundReducerCall)
		{
			const FString ErrorMessage = FString::Printf(
				TEXT("Reducer result for unknown request_id %u"),
				Payload.RequestId);
			HandleProtocolViolation(ErrorMessage);
			return;
		}

		const FReducerCallInfoType ReducerCall = *FoundReducerCall;
		PendingReducerCalls.Remove(Payload.RequestId);

			FReducerEvent RedEvent;
			RedEvent.RequestId = Payload.RequestId;
			RedEvent.Timestamp = Payload.Timestamp;
			RedEvent.CallerIdentity = Identity;
			RedEvent.CallerConnectionId = ConnectionId;
			RedEvent.ReducerCall = ReducerCall;

		if (Payload.Result.IsOk())
		{
			RedEvent.Status = FSpacetimeDBStatus::Committed(FSpacetimeDBUnit());
			const FReducerOkType& Ok = Payload.Result.MessageData.Get<FReducerOkType>();
			const FDatabaseUpdateType Update = TransactionUpdateToDatabaseUpdate(Ok.TransactionUpdate);
			DbUpdate(Update, FSpacetimeDBEvent::Reducer(RedEvent));
			ReducerEvent(RedEvent);
		}
		else if (Payload.Result.IsOkEmpty())
		{
			RedEvent.Status = FSpacetimeDBStatus::Committed(FSpacetimeDBUnit());
			ReducerEvent(RedEvent);
		}
		else
		{
			FString ErrorMessage;
			if (Payload.Result.IsErr())
			{
				ErrorMessage = DecodeReducerErrorMessage(Payload.Result.MessageData.Get<TArray<uint8>>());
			}
			else
			{
				ErrorMessage = Payload.Result.MessageData.Get<FString>();
			}
			RedEvent.Status = FSpacetimeDBStatus::Failed(ErrorMessage);
			ReducerEvent(RedEvent);
			ReducerEventFailed(RedEvent, ErrorMessage);
		}
		break;
	}
	case EServerMessageTag::ProcedureResult:
	{
		const FProcedureResultType& Payload = Message.MessageData.Get<FProcedureResultType>();
		FProcedureEvent ProcEvent;
		ProcEvent.Status = Payload.Status;
		ProcEvent.Timestamp = Payload.Timestamp;
		ProcEvent.TotalHostExecutionDuration = Payload.TotalHostExecutionDuration;
		ProcEvent.Success = ProcEvent.Status.IsReturned();

		TArray<uint8> PayloadData;
		FString ErrorMessage;
		if (ProcEvent.Success)
		{
			PayloadData = ProcEvent.Status.GetAsReturned();
		}
		else if (Payload.Status.IsInternalError())
		{
			ErrorMessage = Payload.Status.GetAsInternalError();
		}

		const bool bResolved = ProcedureCallbacks->ResolveCallback(
			Payload.RequestId,
			FSpacetimeDBEvent::Procedure(ProcEvent),
			PayloadData,
			ProcEvent.Success
		);
		if (!bResolved)
		{
			UE_LOG(
				LogSpacetimeDb_Connection,
				Warning,
				TEXT("Received ProcedureResult for unknown request ID: %u"),
				Payload.RequestId
			);
		}
		if (!ProcEvent.Success)
		{
			ProcedureEventFailed(ProcEvent, ErrorMessage);
		}
		break;
	}
	default:
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("Unknown server-message tag"));
		break;
	}
}

bool UDbConnectionBase::DecompressBrotli(const uint8* InData, int32 InSize, TArray<uint8>& OutData)
{
	(void)InData;
	(void)InSize;
	(void)OutData;
	UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Brotli decompression unavilable"));
	return false;
}

bool UDbConnectionBase::DecompressGzip(const uint8* InData, int32 InSize, TArray<uint8>& OutData)
{
	if (InData == nullptr || InSize < GzipFooterUncompressedSizeBytes)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Gzip data too small"));
		return false;
	}

	// Gzip data ends with 4 bytes indicating the uncompressed size
	const uint8* SizePtr = InData + InSize - GzipFooterUncompressedSizeBytes;
	const uint32 OutSize =
		static_cast<uint32>(SizePtr[0]) |
		(static_cast<uint32>(SizePtr[1]) << 8) |
		(static_cast<uint32>(SizePtr[2]) << 16) |
		(static_cast<uint32>(SizePtr[3]) << 24);
	if (static_cast<int64>(OutSize) > MaxInboundDecodedPayloadBytes)
	{
		UE_LOG(LogSpacetimeDb_Connection,
			Error,
			TEXT("Gzip payload declares %u decoded bytes, exceeding max decoded bytes %lld"),
			OutSize,
			MaxInboundDecodedPayloadBytes);
		return false;
	}

	// Validate the output size
	OutData.SetNumUninitialized(OutSize);
	// Attempt to decompress the Gzip data
	if (!FCompression::UncompressMemory(NAME_Gzip, OutData.GetData(), OutSize, InData, InSize))
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Gzip decompression failed"));
		return false;
	}

	OutData.SetNum(OutSize);
	return true;
}

void UDbConnectionBase::ClearPendingOperations(const FString& Reason)
{
	PendingReducerCalls.Empty();
	if (ProcedureCallbacks)
	{
		ProcedureCallbacks->ClearAllCallbacks();
	}
	if (!Reason.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("Cleared pending operations due to connection issue: %s"), *Reason);
	}
}

void UDbConnectionBase::PreProcessTableUpdateRows(
	const FString& TableName,
	const TArray<FTableUpdateRowsType>& RowSets,
	FPreprocessedTableDataMap& OutPreprocessedTableData)
{
	TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer> Deserializer = FindTableDeserializerForPreprocess(TableName);
	if (!Deserializer.IsValid())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Skipping table %s updates due to missing deserializer"), *TableName);
		return;
	}

	StorePreprocessedTableData(TableName, Deserializer->PreProcess(RowSets, TableName), OutPreprocessedTableData);
}

void UDbConnectionBase::PreProcessQueryRows(
	const FQueryRowsType& Rows,
	UE::SpacetimeDB::EQueryRowsApplyMode Mode,
	FPreprocessedTableDataMap& OutPreprocessedTableData)
{
	for (const FSingleTableRowsType& TableRows : Rows.Tables)
	{
		TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer> Deserializer = FindTableDeserializerForPreprocess(TableRows.Table);
		if (!Deserializer.IsValid())
		{
			UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Skipping table %s query rows due to missing deserializer"), *TableRows.Table);
			continue;
		}

		StorePreprocessedTableData(TableRows.Table, Deserializer->PreProcessQueryRows(TableRows.Rows, Mode, TableRows.Table), OutPreprocessedTableData);
	}
}

void UDbConnectionBase::PreProcessTransactionUpdate(
	const FTransactionUpdateType& Update,
	FPreprocessedTableDataMap& OutPreprocessedTableData)
{
	for (const FQuerySetUpdateType& QuerySet : Update.QuerySets)
	{
		for (const FTableUpdateType& TableUpdate : QuerySet.Tables)
		{
			PreProcessTableUpdateRows(TableUpdate.TableName, TableUpdate.Rows, OutPreprocessedTableData);
		}
	}
}

TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer> UDbConnectionBase::FindTableDeserializerForPreprocess(const FString& TableName)
{
	FScopeLock Lock(&TableDeserializersMutex);
	if (TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer>* Found = TableDeserializers.Find(TableName))
	{
		return *Found;
	}
	return nullptr;
}

void UDbConnectionBase::StorePreprocessedTableData(
	const FString& TableName,
	TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase> Data,
	FPreprocessedTableDataMap& OutPreprocessedTableData)
{
	checkf(Data.IsValid(), TEXT("Invalid message-scoped preprocessed data generated for table '%s'."), *TableName);

	FPreprocessedTableKey Key(TableName);
	TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>& Queue = OutPreprocessedTableData.FindOrAdd(Key);
	Queue.Add(MoveTemp(Data));
}

bool UDbConnectionBase::PreProcessMessage(const TArray<uint8>& Message, FInboundParsedMessage& OutMessage)
{
	TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_PreProcessMessage);

	if (Message.Num() <= SpacetimeDbCompressionTagBytes)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Empty message received from server, ignored"));
		return false;
	}
	// The first byte indicates compression format for the payload.
	const uint8 Compression = Message[0];

	const uint8* DecodedPayload = Message.GetData() + SpacetimeDbCompressionTagBytes;
	int32 DecodedPayloadSize = Message.Num() - SpacetimeDbCompressionTagBytes;
	TArray<uint8> DecompressedStorage;
	switch (static_cast<EWsCompressionTag>(Compression))
	{
	case EWsCompressionTag::Uncompressed:
		break;
	case EWsCompressionTag::Brotli:
		if (!DecompressBrotli(DecodedPayload, DecodedPayloadSize, DecompressedStorage))
		{
			UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Failed to decompress Brotli incoming message"));
			return false;
		}
		DecodedPayload = DecompressedStorage.GetData();
		DecodedPayloadSize = DecompressedStorage.Num();
		break;
	case EWsCompressionTag::Gzip:
		if (!DecompressGzip(DecodedPayload, DecodedPayloadSize, DecompressedStorage))
		{
			UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Failed to decompress Gzip incoming message"));
			return false;
		}
		DecodedPayload = DecompressedStorage.GetData();
		DecodedPayloadSize = DecompressedStorage.Num();
		break;
	default:
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Unknown compression variant"));
		return false;
	}
	if (static_cast<int64>(DecodedPayloadSize) > MaxInboundDecodedPayloadBytes)
	{
		UE_LOG(LogSpacetimeDb_Connection,
			Error,
			TEXT("Decoded server message payload has %d bytes, exceeding max decoded bytes %lld after compression tag %u."),
			DecodedPayloadSize,
			MaxInboundDecodedPayloadBytes,
			static_cast<uint32>(Compression));
		return false;
	}
	if (DecodedPayloadSize <= 0 || DecodedPayload == nullptr)
	{
		UE_LOG(LogSpacetimeDb_Connection,
			Error,
			TEXT("SpacetimeDB decoded server message payload is empty after compression tag %u."),
			static_cast<uint32>(Compression));
		return false;
	}

	OutMessage.DecodedPayloadSizeBytes = DecodedPayloadSize;
	// Deserialize the decompressed data into a UServerMessageType object
	OutMessage.Message = UE::SpacetimeDB::DeserializeView<FServerMessageType>(DecodedPayload, DecodedPayloadSize);

	// Preprocess row-bearing payloads for table deserializers.
	switch (OutMessage.Message.Tag)
	{
		case EServerMessageTag::SubscribeApplied:
		{
			const FSubscribeAppliedType& Payload = OutMessage.Message.MessageData.Get<FSubscribeAppliedType>();
			PreProcessQueryRows(Payload.Rows, UE::SpacetimeDB::EQueryRowsApplyMode::Inserts, OutMessage.PreprocessedTableData);
			break;
		}
		case EServerMessageTag::UnsubscribeApplied:
		{
			const FUnsubscribeAppliedType& Payload = OutMessage.Message.MessageData.Get<FUnsubscribeAppliedType>();
			if (Payload.Rows.IsSet())
			{
				PreProcessQueryRows(Payload.Rows.Value, UE::SpacetimeDB::EQueryRowsApplyMode::Deletes, OutMessage.PreprocessedTableData);
			}
			break;
		}
		case EServerMessageTag::TransactionUpdate:
		{
			const FTransactionUpdateType& Payload = OutMessage.Message.MessageData.Get<FTransactionUpdateType>();
			PreProcessTransactionUpdate(Payload, OutMessage.PreprocessedTableData);
			break;
		}
		case EServerMessageTag::ReducerResult:
		{
			const FReducerResultType& Payload = OutMessage.Message.MessageData.Get<FReducerResultType>();
			if (Payload.Result.IsOk())
			{
				const FReducerOkType& Ok = Payload.Result.MessageData.Get<FReducerOkType>();
				PreProcessTransactionUpdate(Ok.TransactionUpdate, OutMessage.PreprocessedTableData);
			}
			break;
		}
		default:
			break;
	}
	OutMessage.EstimatedMemoryBytes = EstimateInboundParsedMessageBytes(OutMessage);
	return true;
}


uint32 UDbConnectionBase::GetNextRequestId()
{
	return NextRequestId++;
}

uint32 UDbConnectionBase::GetNextSubscriptionId()
{
	return NextSubscriptionId++;
}

void UDbConnectionBase::StartSubscription(USubscriptionHandleBase* Handle)
{
	if (!Handle)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("StartSubscription called with null handle"));
		return;
	}

	if (Handle->QuerySqls.Num() == 0)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("StartSubscription called with empty query list"));
		return;
	}

	const uint32 QuerySetId = GetNextSubscriptionId();
	Handle->QuerySetId = QuerySetId;
	Handle->ConnInternal = this;
	ActiveSubscriptions.Add(QuerySetId, Handle);

	FSubscribeType SubMsg;
	SubMsg.RequestId = GetNextRequestId();
	SubMsg.QuerySetId.Id = QuerySetId;
	SubMsg.QueryStrings = Handle->QuerySqls;

	FClientMessageType Msg = FClientMessageType::Subscribe(SubMsg);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
}

void UDbConnectionBase::UnsubscribeInternal(USubscriptionHandleBase* Handle)
{
	if (!Handle || Handle->bEnded)
	{
		return;
	}

	const uint32 QuerySetId = Handle->QuerySetId;
	FUnsubscribeType MsgData;
	MsgData.RequestId = GetNextRequestId();
	MsgData.QuerySetId.Id = QuerySetId;
	MsgData.Flags = EUnsubscribeFlagsType::SendDroppedRows;

	FClientMessageType Msg = FClientMessageType::Unsubscribe(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
}

uint32 UDbConnectionBase::InternalCallReducer(const FString& Reducer, TArray<uint8> Args)
{
	if (!WebSocket || !WebSocket->IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Cannot call reducer, not connected to server!"));
		return 0;
	}

	FCallReducerType MsgData;
	MsgData.Reducer = Reducer;
	MsgData.Args = Args;
	MsgData.RequestId = GetNextRequestId();
	// v2 parity with Rust/C#: reducer flags are always default.
	MsgData.Flags = 0;
	FReducerCallInfoType CallInfo;
	CallInfo.ReducerName = Reducer;
	CallInfo.Args = Args;
	PendingReducerCalls.Add(MsgData.RequestId, CallInfo);

	FClientMessageType Msg = FClientMessageType::CallReducer(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
	return MsgData.RequestId;
}

void UDbConnectionBase::InternalCallProcedure(const FString& ProcedureName, TArray<uint8> Args, const FOnProcedureCompleteDelegate& Callback)
{
	if (!WebSocket || !WebSocket->IsConnected())
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Cannot call proceduer, not connected to server!"));
		return;
	}
	FCallProcedureType MsgData;
	MsgData.Procedure = ProcedureName;
	MsgData.Args = Args;
	MsgData.RequestId = ProcedureCallbacks->RegisterCallback(Callback);
	MsgData.Flags = static_cast<uint8>(EProcedureFlags::Default);

	FClientMessageType Msg = FClientMessageType::CallProcedure(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
}

void UDbConnectionBase::ApplyRegisteredTableUpdates(const FDatabaseUpdateType& Update, void* Context)
{
	checkf(ActivePreprocessedTableData != nullptr, TEXT("ApplyRegisteredTableUpdates requires active message-scoped preprocessed data."));

	TSharedPtr<const TMap<FString, TSharedPtr<ITableUpdateHandler>>> RegisteredHandlers;
	{
		FScopeLock Lock(&RegisteredTablesMutex);
		RegisteredHandlers = RegisteredTablesSnapshot;
	}
	if (!RegisteredHandlers.IsValid())
	{
		return;
	}

	TableUpdateHandlersScratch.Reset();
	TableUpdateHandlersScratch.Reserve(Update.Tables.Num());
	if (ActiveInboundMessageApplyStats != nullptr)
	{
		ActiveInboundMessageApplyStats->TableStats.Reserve(
			ActiveInboundMessageApplyStats->TableStats.Num() + Update.Tables.Num());
	}
	for (const FTableUpdateType& TableUpdate : Update.Tables)
	{
		if (TableUpdate.Rows.IsEmpty())
		{
			continue;
		}

		TSharedPtr<ITableUpdateHandler> Handler;
		if (const TSharedPtr<ITableUpdateHandler>* Found = RegisteredHandlers->Find(TableUpdate.TableName))
		{
			Handler = *Found;
		}
		if (Handler.IsValid())
		{
			FSpacetimeDBTableApplyStats* TableStats = nullptr;
			int32 TableStatsIndex = INDEX_NONE;
			if (ActiveInboundMessageApplyStats != nullptr)
			{
				TableStatsIndex = ActiveInboundMessageApplyStats->TableStats.AddDefaulted();
				TableStats = &ActiveInboundMessageApplyStats->TableStats[TableStatsIndex];
				TableStats->TableName = Handler->GetTableName();
			}

			TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_TableUpdateCache);
			const uint64 TableCacheStartCycles = FPlatformTime::Cycles64();
			const bool bHasNonEmptyDiff = Handler->UpdateCache(this, TableUpdate, Context, TableStats);
			const double TableCacheElapsedMicros =
				FPlatformTime::ToMilliseconds64(FPlatformTime::Cycles64() - TableCacheStartCycles) * 1000.0;
			if (TableStats != nullptr)
			{
				TableStats->CacheMicros = TableCacheElapsedMicros;
			}
			if (InboundApplyBudget.SoftTimeBudgetMicros > 0 &&
				TableCacheElapsedMicros >= static_cast<double>(InboundApplyBudget.SoftTimeBudgetMicros))
			{
				UE_LOG(LogSpacetimeDb_Connection,
					Warning,
					TEXT("SpacetimeDB table cache apply exceeded soft budget: table=%s elapsed=%.2fus budget=%lldus row_ops=%d"),
					*Handler->GetTableName(),
					TableCacheElapsedMicros,
					InboundApplyBudget.SoftTimeBudgetMicros,
					TableUpdate.Rows.Num());
			}
			if (bHasNonEmptyDiff)
			{
				FPendingTableBroadcast& PendingBroadcast = TableUpdateHandlersScratch.AddDefaulted_GetRef();
				PendingBroadcast.Handler = Handler;
				PendingBroadcast.StatsIndex = TableStatsIndex;
			}
		}
	}

	for (FPendingTableBroadcast& PendingBroadcast : TableUpdateHandlersScratch)
	{
		TSharedPtr<ITableUpdateHandler>& Handler = PendingBroadcast.Handler;
		checkf(Handler.IsValid(), TEXT("Invalid pending SpacetimeDB table broadcast handler."));
		TRACE_CPUPROFILER_EVENT_SCOPE(SpacetimeDB_TableBroadcastDiff);
		const uint64 BroadcastStartCycles = FPlatformTime::Cycles64();
		Handler->BroadcastDiff(this, Context);
		const double BroadcastElapsedMicros =
			FPlatformTime::ToMilliseconds64(FPlatformTime::Cycles64() - BroadcastStartCycles) * 1000.0;
		if (ActiveInboundMessageApplyStats != nullptr && PendingBroadcast.StatsIndex != INDEX_NONE)
		{
			checkf(ActiveInboundMessageApplyStats->TableStats.IsValidIndex(PendingBroadcast.StatsIndex),
				TEXT("Invalid SpacetimeDB inbound apply stats index %d."),
				PendingBroadcast.StatsIndex);
			ActiveInboundMessageApplyStats->TableStats[PendingBroadcast.StatsIndex].BroadcastMicros = BroadcastElapsedMicros;
		}
		if (InboundApplyBudget.SoftTimeBudgetMicros > 0 &&
			BroadcastElapsedMicros >= static_cast<double>(InboundApplyBudget.SoftTimeBudgetMicros))
		{
			UE_LOG(LogSpacetimeDb_Connection,
				Warning,
				TEXT("SpacetimeDB table broadcast exceeded soft budget: table=%s elapsed=%.2fus budget=%lldus"),
				*Handler->GetTableName(),
				BroadcastElapsedMicros,
				InboundApplyBudget.SoftTimeBudgetMicros);
		}
	}
	TableUpdateHandlersScratch.Reset();
}
