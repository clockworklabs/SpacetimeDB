#include "Connection/DbConnectionBase.h"
#include "Connection/DbConnectionBuilder.h"
#include "Connection/Credentials.h"
#include "Connection/LogCategory.h"
#include "Containers/Ticker.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "Misc/Compression.h"
#include "Misc/ScopeLock.h"
#include "Async/Async.h"
#include "BSATN/UEBSATNHelpers.h"
#include "Connection/ProcedureFlags.h"
#include "Misc/Base64.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonSerializer.h"
#include "HAL/PlatformTime.h"
#include <exception>
#include <stdexcept>
#include "Internationalization/Regex.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"

namespace
{
	enum class EWsCompressionTag : uint8
	{
		Uncompressed = 0,
		Brotli = 1,
		Gzip = 2,
	};

	static FDatabaseUpdateType QueryRowsToDatabaseUpdate(const FQueryRowsType& Rows, bool bAsDeletes)
	{
		FDatabaseUpdateType Update;
		for (const FSingleTableRowsType& TableRows : Rows.Tables)
		{
			FTableUpdateType TableUpdate;
			TableUpdate.TableName = TableRows.Table;

			FPersistentTableRowsType PersistentRows;
			if (bAsDeletes)
			{
				PersistentRows.Deletes = TableRows.Rows;
			}
			else
			{
				PersistentRows.Inserts = TableRows.Rows;
			}
			TableUpdate.Rows.Add(FTableUpdateRowsType::PersistentTable(PersistentRows));
			Update.Tables.Add(TableUpdate);
		}
		return Update;
	}

	static FDatabaseUpdateType TransactionUpdateToDatabaseUpdate(const FTransactionUpdateType& Update)
	{
		FDatabaseUpdateType Out;
		TMap<FString, int32> TableNameToIndex;
		for (const FQuerySetUpdateType& QuerySet : Update.QuerySets)
		{
			for (const FTableUpdateType& TableUpdate : QuerySet.Tables)
			{
				if (const int32* ExistingIndex = TableNameToIndex.Find(TableUpdate.TableName))
				{
					Out.Tables[*ExistingIndex].Rows.Append(TableUpdate.Rows);
				}
				else
				{
					const int32 NewIndex = Out.Tables.Add(TableUpdate);
					TableNameToIndex.Add(TableUpdate.TableName, NewIndex);
				}
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
}

UDbConnectionBase::UDbConnectionBase(const FObjectInitializer& ObjectInitializer) : Super(ObjectInitializer)
{
	NextRequestId = 1;
	NextSubscriptionId = 1;
	ProcedureCallbacks = CreateDefaultSubobject<UProcedureCallbacks>(TEXT("ProcedureCallbacks"));
}

void UDbConnectionBase::SetAutoTicking(bool bAutoTick)
{
	if (bIsAutoTicking == bAutoTick)
	{
		return;
	}

	bIsAutoTicking = bAutoTick;

	if (bIsAutoTicking)
	{
		if (!TickerHandle.IsValid())
		{
			TickerHandle = FTSTicker::GetCoreTicker().AddTicker(
			    FTickerDelegate::CreateUObject(this, &UDbConnectionBase::OnTickerTick));
		}
	}
	else if (TickerHandle.IsValid())
	{
		FTSTicker::GetCoreTicker().RemoveTicker(TickerHandle);
		TickerHandle.Reset();
	}
}

void UDbConnectionBase::BeginDestroy()
{
	if (TickerHandle.IsValid())
	{
		FTSTicker::GetCoreTicker().RemoveTicker(TickerHandle);
		TickerHandle.Reset();
	}
	bIsAutoTicking = false;
	bEnded = true;
	ResetTransport();

	Super::BeginDestroy();
}

void UDbConnectionBase::Disconnect()
{
	if (bEnded)
	{
		return;
	}
	bEnded = true;
	bEstablished = false;
	bReconnecting = false;
	bAttemptPending = false;
	ResetTransport();
	SuspendSubscriptions(true);
	ClearPendingOperations(TEXT("Connection closed before the result was received"));
	OnDisconnectBaseDelegate.ExecuteIfBound(this, FString(), false, 0, 0.0f);
}

bool UDbConnectionBase::IsActive() const
{
	return !bEnded && !bReconnecting && WebSocket && WebSocket->IsConnected();
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
	return IsActive() && WebSocket->SendMessage(Message);
}

bool UDbConnectionBase::SendRawMessage(const TArray<uint8>& Message)
{
	return IsActive() && WebSocket->SendMessage(Message);
}

USubscriptionBuilderBase* UDbConnectionBase::SubscriptionBuilderBase()
{
	return NewObject<USubscriptionBuilderBase>();
}

void UDbConnectionBase::QueueFailure(const FString& Error, bool bTerminal, bool bAuth)
{
	if (bEnded)
	{
		return;
	}
	FIncomingEvent Event;
	Event.Error = Error;
	Event.bFailure = true;
	Event.bTerminal = bTerminal;
	Event.bAuth = bAuth;
	IncomingEvents.Add(NextIncomingId++, MoveTemp(Event));
}

void UDbConnectionBase::HandleWSError(const FString& Error)
{
	// Unreal's WebSocket API exposes HTTP upgrade failures as text, not status codes.
	// Match a status in an HTTP diagnostic, never digits in a URL or port.
	const FRegexPattern StatusPattern(TEXT("(?i)(?:HTTP(?:/[0-9.]+)?|status(?: code)?|response(?: code)?|ws upgrade "
	                                       "not 101, was)[^0-9\\r\\n]{0,24}(400|401|403)(?:[^0-9]|$)"));
	FRegexMatcher StatusMatch(StatusPattern, Error);
	const bool bAuth = StatusMatch.FindNext();
	QueueFailure(Error, false, bAuth);
}

void UDbConnectionBase::HandleWSClosed(int32 StatusCode, const FString& Reason, bool /*bWasClean*/)
{
	const bool bTerminal = StatusCode == 1002 || StatusCode == 1003 || StatusCode == 1007 || StatusCode == 1008;
	QueueFailure(Reason.IsEmpty() ? TEXT("Connection closed") : Reason, bTerminal);
}

void UDbConnectionBase::HandleProtocolViolation(const FString& ErrorMessage)
{
	if (bEnded)
	{
		return;
	}
	UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("%s"), *ErrorMessage);
	ConnectionLost(ErrorMessage, true);
}

void UDbConnectionBase::HandleWSBinaryMessage(const TArray<uint8>& Message)
{
	if (bEnded)
	{
		return;
	}
	const uint64 Id = NextIncomingId++;
	const uint64 Generation = ConnectionGeneration;
	TWeakObjectPtr<UDbConnectionBase> WeakThis(this);
	const FDeserializerMap Deserializers = TableDeserializers;
	// Parsing owns only bytes and value types. Never dereference a UObject on the worker.
	Async(
	    EAsyncExecution::ThreadPool,
	    [WeakThis, Message, Id, Generation, Deserializers]()
	    {
		    FIncomingEvent Event;
		    try
		    {
			    Event.bFailure = !PreProcessMessage(Message, Event.Message);
			    if (!Event.bFailure)
			    {
				    // Each message owns its decoded rows, so out-of-order workers cannot
				    // consume another message's rows or contaminate a replacement socket.
				    switch (Event.Message.Tag)
				    {
					    case EServerMessageTag::SubscribeApplied:
						    PreProcessDatabaseUpdate(
						        QueryRowsToDatabaseUpdate(Event.Message.GetAsSubscribeApplied().Rows, false),
						        Deserializers, Event.TableData);
						    break;
					    case EServerMessageTag::UnsubscribeApplied:
						    if (Event.Message.GetAsUnsubscribeApplied().Rows.IsSet())
						    {
							    PreProcessDatabaseUpdate(
							        QueryRowsToDatabaseUpdate(Event.Message.GetAsUnsubscribeApplied().Rows.Value, true),
							        Deserializers, Event.TableData);
						    }
						    break;
					    case EServerMessageTag::TransactionUpdate:
						    PreProcessDatabaseUpdate(
						        TransactionUpdateToDatabaseUpdate(Event.Message.GetAsTransactionUpdate()),
						        Deserializers, Event.TableData);
						    break;
					    case EServerMessageTag::ReducerResult:
						    if (Event.Message.GetAsReducerResult().Result.IsOk())
						    {
							    PreProcessDatabaseUpdate(
							        TransactionUpdateToDatabaseUpdate(
							            Event.Message.GetAsReducerResult().Result.GetAsOk().TransactionUpdate),
							        Deserializers, Event.TableData);
						    }
						    break;
					    default:
						    break;
				    }
			    }
		    }
		    catch (const std::exception&)
		    {
			    Event.bFailure = true;
		    }
		    if (Event.bFailure)
		    {
			    Event.bTerminal = true;
			    Event.Error = TEXT("Failed to parse/decompress incoming WebSocket message");
		    }
		    AsyncTask(ENamedThreads::GameThread,
		              [WeakThis, Id, Generation, Event = MoveTemp(Event)]() mutable
		              {
			              if (WeakThis.IsValid() && WeakThis->ConnectionGeneration == Generation && !WeakThis->bEnded)
			              {
				              WeakThis->AcceptIncomingEvent(Generation, Id, MoveTemp(Event));
			              }
		              });
	    });
}

void UDbConnectionBase::FrameTick()
{
	if (bTicking || bEnded)
	{
		return;
	}
	TGuardValue<bool> TickGuard(bTicking, true);
	const uint64 Generation = ConnectionGeneration;
	FIncomingEvent Event;
	while (IncomingEvents.RemoveAndCopyValue(NextReleaseId, Event))
	{
		++NextReleaseId;
		if (Event.bFailure)
		{
			ConnectionLost(Event.Error, Event.bTerminal, Event.bAuth);
		}
		else
		{
			PreprocessedTableData = MoveTemp(Event.TableData);
			try
			{
				ProcessServerMessage(Event.Message);
			}
			catch (const std::exception&)
			{
				HandleProtocolViolation(TEXT("Invalid server message or row data"));
			}
		}
		if (bEnded || ConnectionGeneration != Generation)
		{
			break;
		}
	}
	AdvanceReconnect();
}

bool UDbConnectionBase::OnTickerTick(float DeltaTime)
{
	if (HasAnyFlags(RF_BeginDestroyed | RF_FinishDestroyed) || !bIsAutoTicking)
	{
		return false;
	}

	FrameTick();
	return true;
}

void UDbConnectionBase::ProcessServerMessage(const FServerMessageType& Message)
{
	if (bEnded)
	{
		return;
	}
	if (!bEstablished && Message.Tag != EServerMessageTag::InitialConnection)
	{
		HandleProtocolViolation(TEXT("Expected InitialConnection before data messages"));
		return;
	}
	switch (Message.Tag)
	{
		case EServerMessageTag::InitialConnection:
		{
			const FInitialConnectionType Payload = Message.GetAsInitialConnection();
			if (bEstablished || (bIsIdentitySet && Identity != Payload.Identity))
			{
				HandleProtocolViolation(bEstablished ? TEXT("Duplicate InitialConnection")
				                                     : TEXT("Reconnect returned a different identity"));
				return;
			}
			const bool bWasReconnecting = bReconnecting;
			Token = Payload.Token;
			// Keep the SDK's existing persistence behavior; reconnects always reuse this token in memory.
			UCredentials::SaveToken(Token);
			Identity = Payload.Identity;
			bIsIdentitySet = true;
			ConnectionId = Payload.ConnectionId;
			bEstablished = true;
			bAttemptPending = false;
			bReconnecting = false;
			ReconnectAttempt = 0;
			bPreparingReplay = bWasReconnecting;
			const uint64 Generation = ConnectionGeneration;
			if (bWasReconnecting)
			{
				OnAutomaticReconnectBaseDelegate.ExecuteIfBound(this, Identity, Token);
			}
			else
			{
				OnConnectBaseDelegate.ExecuteIfBound(this, Identity, Token);
			}
			if (bWasReconnecting && !bEnded && ConnectionGeneration == Generation)
			{
				ReplaySubscriptions();
			}
			break;
		}
		case EServerMessageTag::SubscribeBatchApplied:
			ApplyReplay(Message.GetAsSubscribeBatchApplied());
			break;
		case EServerMessageTag::TransactionUpdate:
		{
			const FTransactionUpdateType Payload = Message.GetAsTransactionUpdate();
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
			const FSubscribeAppliedType Payload = Message.GetAsSubscribeApplied();
			const FDatabaseUpdateType Update = QueryRowsToDatabaseUpdate(Payload.Rows, false);
			DbUpdate(Update, FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit()));

			if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
			{
				TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
				if (!Handle)
				{
					UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("SubscribeApplied: Null handle for QuerySetId %u."),
					       Payload.QuerySetId.Id);
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
			const FUnsubscribeAppliedType Payload = Message.GetAsUnsubscribeApplied();
			if (Payload.Rows.IsSet())
			{
				const FDatabaseUpdateType Update = QueryRowsToDatabaseUpdate(Payload.Rows.Value, true);
				DbUpdate(Update, FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit()));
			}

			if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
			{
				TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
				if (!Handle)
				{
					UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("UnsubscribeApplied: Null handle for QuerySetId %u."),
					       Payload.QuerySetId.Id);
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
			const FSubscriptionErrorType Payload = Message.GetAsSubscriptionError();
			UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("SubscriptionError received for QuerySetId=%u Error=%s"),
			       Payload.QuerySetId.Id, *Payload.Error);
			if (TObjectPtr<USubscriptionHandleBase>* HandlePtr = ActiveSubscriptions.Find(Payload.QuerySetId.Id))
			{
				TObjectPtr<USubscriptionHandleBase> Handle = *HandlePtr;
				if (!Handle)
				{
					UE_LOG(LogSpacetimeDb_Connection, Error,
					       TEXT("SubscriptionError: Null handle for QuerySetId %u. Error: %s"), Payload.QuerySetId.Id,
					       *Payload.Error);
					return;
				}
				FErrorContextBase Ctx;
				Ctx.Error = Payload.Error;
				ActiveSubscriptions.Remove(Payload.QuerySetId.Id);
				Handle->TriggerError(Ctx);
			}
			break;
		}
		case EServerMessageTag::ReducerResult:
		{
			const FReducerResultType Payload = Message.GetAsReducerResult();
			const FReducerCallInfoType* FoundReducerCall = PendingReducerCalls.Find(Payload.RequestId);
			if (!FoundReducerCall)
			{
				const FString ErrorMessage =
				    FString::Printf(TEXT("Reducer result for unknown request_id %u"), Payload.RequestId);
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
				const FReducerOkType Ok = Payload.Result.GetAsOk();
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
					ErrorMessage = DecodeReducerErrorMessage(Payload.Result.GetAsErr());
				}
				else
				{
					ErrorMessage = Payload.Result.GetAsInternalError();
				}
				RedEvent.Status = FSpacetimeDBStatus::Failed(ErrorMessage);
				ReducerEvent(RedEvent);
				ReducerEventFailed(RedEvent, ErrorMessage);
			}
			break;
		}
		case EServerMessageTag::ProcedureResult:
		{
			const FProcedureResultType Payload = Message.GetAsProcedureResult();
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
			    Payload.RequestId, FSpacetimeDBEvent::Procedure(ProcEvent), PayloadData, ProcEvent.Success);
			if (!bResolved)
			{
				UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("Received ProcedureResult for unknown request ID: %u"),
				       Payload.RequestId);
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

bool UDbConnectionBase::DecompressBrotli(const TArray<uint8>& InData, TArray<uint8>& OutData)
{
	UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Brotli decompression unavilable"));
	return false;
}

bool UDbConnectionBase::DecompressGzip(const TArray<uint8>& InData, TArray<uint8>& OutData)
{
	if (InData.Num() < 4)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Gzip data too small"));
		return false;
	}

	// Gzip data ends with 4 bytes indicating the uncompressed size
	const uint8* SizePtr = InData.GetData() + InData.Num() - 4;
	uint32 OutSize = SizePtr[0] | (SizePtr[1] << 8) | (SizePtr[2] << 16) | (SizePtr[3] << 24);

	// Validate the output size
	OutData.SetNumUninitialized(OutSize);
	// Attempt to decompress the Gzip data
	if (!FCompression::UncompressMemory(NAME_Gzip, OutData.GetData(), OutSize, InData.GetData(), InData.Num()))
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Gzip decompression failed"));
		return false;
	}

	OutData.SetNum(OutSize);
	return true;
}

bool UDbConnectionBase::DecompressPayload(uint8 Variant, const TArray<uint8>& In, TArray<uint8>& Out)
{
	switch (static_cast<EWsCompressionTag>(Variant))
	{
		case EWsCompressionTag::Uncompressed:
			// No compression, just copy the data
			Out = In;
			return true;
		case EWsCompressionTag::Brotli:
			return DecompressBrotli(In, Out);
		case EWsCompressionTag::Gzip:
			return DecompressGzip(In, Out);
		default:
			UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Unknown compression variant"));
			return false;
	}
}

void UDbConnectionBase::ClearPendingOperations(const FString& Reason)
{
	auto Calls = MoveTemp(PendingReducerCalls);
	PendingReducerCalls.Empty();
	if (bAutomaticReconnect)
	{
		for (const auto& Pair : Calls)
		{
			FReducerEvent Event;
			Event.RequestId = Pair.Key;
			Event.ReducerCall = Pair.Value;
			Event.CallerIdentity = Identity;
			Event.CallerConnectionId = ConnectionId;
			Event.Status = FSpacetimeDBStatus::Failed(Reason);
			Event.CallError = ESpacetimeDBCallError::UnknownResult;
			ReducerEvent(Event);
			ReducerEventFailed(Event, Reason);
		}
		FProcedureEvent Event;
		Event.CallError = ESpacetimeDBCallError::UnknownResult;
		Event.Status = FProcedureStatusType::InternalError(Reason);
		ProcedureCallbacks->FailAllCallbacks(FSpacetimeDBEvent::Procedure(Event));
	}
	else if (ProcedureCallbacks)
	{
		ProcedureCallbacks->ClearAllCallbacks();
	}

	if (!Reason.IsEmpty())
	{
		UE_LOG(LogSpacetimeDb_Connection, Warning, TEXT("Cleared pending operations due to connection issue: %s"),
		       *Reason);
	}
}

void UDbConnectionBase::PreProcessDatabaseUpdate(const FDatabaseUpdateType& Update,
                                                 const FDeserializerMap& Deserializers, FTableDataMap& Out)
{
	for (const FTableUpdateType& TableUpdate : Update.Tables)
	{
		const auto* Found = Deserializers.Find(TableUpdate.TableName);
		if (!Found)
		{
			throw std::runtime_error("Unknown table in server update");
		}
		auto Data = (*Found)->PreProcess(TableUpdate.Rows, TableUpdate.TableName);
		if (!Data.IsValid())
		{
			throw std::runtime_error("Invalid table update");
		}
		Out.FindOrAdd(FPreprocessedTableKey(TableUpdate.TableName)).Add(MoveTemp(Data));
	}
}

void UDbConnectionBase::PreProcessDatabaseUpdate(const FDatabaseUpdateType& Update)
{
	PreprocessedTableData.Empty();
	PreProcessDatabaseUpdate(Update, TableDeserializers, PreprocessedTableData);
}

bool UDbConnectionBase::PreProcessMessage(const TArray<uint8>& Message, FServerMessageType& OutMessage)
{
	if (Message.Num() == 0)
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Empty message recived from server, ignored"));
		return false;
	}
	// The first byte indicates compression format for the payload.
	const uint8 Compression = Message[0];
	TArray<uint8> CompressedPayload;
	CompressedPayload.Append(Message.GetData() + 1, Message.Num() - 1);

	// Decompress the payload based on the compression tag
	TArray<uint8> Decompressed;
	if (!DecompressPayload(Compression, CompressedPayload, Decompressed))
	{
		UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Failed to decompress incoming message"));
		return false;
	}

	// Deserialize the decompressed data into a UServerMessageType object
	OutMessage = UE::SpacetimeDB::Deserialize<FServerMessageType>(Decompressed);

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

	if (bEnded)
	{
		ActiveSubscriptions.Remove(QuerySetId);
		FErrorContextBase Context;
		Context.Error = TEXT("Disconnected");
		Handle->TriggerError(Context);
		return;
	}
	if (bReconnecting || bPreparingReplay)
	{
		return;
	}

	FSubscribeType SubMsg;
	SubMsg.RequestId = GetNextRequestId();
	SubMsg.QuerySetId.Id = QuerySetId;
	SubMsg.QueryStrings = Handle->QuerySqls;

	FClientMessageType Msg = FClientMessageType::Subscribe(SubMsg);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	if (!SendRawMessage(Data))
	{
		QueueFailure(TEXT("Disconnected while subscribing"), false);
	}
}

void UDbConnectionBase::UnsubscribeInternal(USubscriptionHandleBase* Handle)
{
	if (!Handle || Handle->bEnded)
	{
		return;
	}

	if (bEnded || bReconnecting || bPreparingReplay)
	{
		EndSubscription(Handle);
		return;
	}
	const uint32 QuerySetId = Handle->QuerySetId;
	FUnsubscribeType MsgData;
	MsgData.RequestId = GetNextRequestId();
	MsgData.QuerySetId.Id = QuerySetId;
	MsgData.Flags = EUnsubscribeFlagsType::SendDroppedRows;

	FClientMessageType Msg = FClientMessageType::Unsubscribe(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	if (!SendRawMessage(Data))
	{
		QueueFailure(TEXT("Connection lost while sending a request"), false);
	}
}

uint32 UDbConnectionBase::InternalCallReducer(const FString& Reducer, TArray<uint8> Args)
{
	if (!IsActive())
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
	if (!SendRawMessage(Data))
	{
		QueueFailure(TEXT("Connection lost while sending a reducer"), false);
	}
	return MsgData.RequestId;
}

void UDbConnectionBase::InternalCallProcedure(const FString& ProcedureName, TArray<uint8> Args,
                                              const FOnProcedureCompleteDelegate& Callback)
{
	if (!IsActive())
	{
		FProcedureEvent Event;
		Event.Status = FProcedureStatusType::InternalError(TEXT("Disconnected"));
		Event.CallError = ESpacetimeDBCallError::Disconnected;
		Callback.ExecuteIfBound(FSpacetimeDBEvent::Procedure(Event), TArray<uint8>(), false);
		return;
	}
	FCallProcedureType MsgData;
	MsgData.Procedure = ProcedureName;
	MsgData.Args = Args;
	MsgData.RequestId = ProcedureCallbacks->RegisterCallback(Callback);
	MsgData.Flags = static_cast<uint8>(EProcedureFlags::Default);

	FClientMessageType Msg = FClientMessageType::CallProcedure(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	if (!SendRawMessage(Data))
	{
		QueueFailure(TEXT("Connection lost while sending a request"), false);
	}
}

void UDbConnectionBase::ApplyRegisteredTableUpdates(const FDatabaseUpdateType& Update, void* Context)
{
	// Ensure we have a valid context for the update
	TArray<TSharedPtr<ITableUpdateHandler>> Handlers;
	for (const FTableUpdateType& TableUpdate : Update.Tables)
	{
		TSharedPtr<ITableUpdateHandler> Handler;
		{
			// Find the handler for this table update
			FScopeLock Lock(&RegisteredTablesMutex);
			if (TSharedPtr<ITableUpdateHandler>* Found = RegisteredTables.Find(TableUpdate.TableName))
			{
				Handler = *Found;
			}
		}
		if (Handler.IsValid())
		{
			// Update the cache for the handler with the table update and context
			Handler->UpdateCache(this, TableUpdate, Context);
			Handlers.Add(Handler);
		}
	}

	for (TSharedPtr<ITableUpdateHandler>& Handler : Handlers)
	{
		// Broadcast the diff for each handler
		Handler->BroadcastDiff(this, Context);
	}
}

void UDbConnectionBase::ResetTransport()
{
	++ConnectionGeneration; // Invalidate parses and credential completions from this socket attempt.
	if (TokenRequest)
	{
		TokenRequest->Completion = nullptr;
		TokenRequest = nullptr;
	}
	if (TokenValidation)
	{
		TokenValidation->OnProcessRequestComplete().Unbind();
		TokenValidation->CancelRequest();
		TokenValidation.Reset();
	}
	if (WebSocket)
	{
		WebSocket->OnConnectionError.RemoveAll(this);
		WebSocket->OnClosed.RemoveAll(this);
		WebSocket->OnBinaryMessageReceived.RemoveAll(this);
		WebSocket->Disconnect();
		WebSocket = nullptr;
	}
	IncomingEvents.Empty();
	NextIncomingId = NextReleaseId = 0;
	PreprocessedTableData.Empty();
	ReplayRequestId = 0;
	ReplayQueryIds.Empty();
	bPreparingReplay = false;
}

void UDbConnectionBase::StartSocket()
{
	if (!bAutomaticReconnect || Token.IsEmpty())
	{
		OpenSocket();
		return;
	}
	// IWebSocket does not reliably expose HTTP upgrade status codes (notably on
	// Windows). Validate credentials through the same endpoint the browser SDK
	// uses, so a rejected token can trigger refresh or terminate on every backend.
	FString HttpUri = Uri;
	if (HttpUri.StartsWith(TEXT("wss://")))
	{
		HttpUri = TEXT("https://") + HttpUri.Mid(6);
	}
	else if (HttpUri.StartsWith(TEXT("ws://")))
	{
		HttpUri = TEXT("http://") + HttpUri.Mid(5);
	}
	TokenValidation = FHttpModule::Get().CreateRequest();
	TokenValidation->SetURL(HttpUri + TEXT("/v1/identity/websocket-token"));
	TokenValidation->SetVerb(TEXT("POST"));
	TokenValidation->SetDelegateThreadPolicy(EHttpRequestDelegateThreadPolicy::CompleteOnGameThread);
	TokenValidation->SetHeader(TEXT("Authorization"), TEXT("Bearer ") + Token);
	const uint64 Generation = ConnectionGeneration;
	TWeakObjectPtr<UDbConnectionBase> WeakThis(this);
	TokenValidation->OnProcessRequestComplete().BindLambda(
	    [WeakThis, Generation](FHttpRequestPtr, FHttpResponsePtr Response, bool bSuccess)
	    {
		    if (!WeakThis.IsValid() || WeakThis->bEnded || WeakThis->ConnectionGeneration != Generation)
		    {
			    return;
		    }
		    auto* Conn = WeakThis.Get();
		    Conn->TokenValidation.Reset();
		    const int32 Status = Response.IsValid() ? Response->GetResponseCode() : 0;
		    if (bSuccess && Status >= 200 && Status < 300)
		    {
			    Conn->OpenSocket();
		    }
		    else
		    {
			    const bool bAuth = Status == 400 || Status == 401 || Status == 403;
			    Conn->QueueFailure(FString::Printf(TEXT("Token validation failed (HTTP %d)"), Status), false, bAuth);
		    }
	    });
	if (!TokenValidation->ProcessRequest())
	{
		TokenValidation.Reset();
		QueueFailure(TEXT("Could not start token validation"), false);
	}
}

void UDbConnectionBase::OpenSocket()
{
	WebSocket = NewObject<UWebsocketManager>(this);
	WebSocket->OnConnectionError.AddDynamic(this, &UDbConnectionBase::HandleWSError);
	WebSocket->OnClosed.AddDynamic(this, &UDbConnectionBase::HandleWSClosed);
	WebSocket->OnBinaryMessageReceived.AddDynamic(this, &UDbConnectionBase::HandleWSBinaryMessage);
	WebSocket->SetInitToken(Token);
	WebSocket->Connect(WebSocketUrl);
}

void UDbConnectionBase::ConnectionLost(const FString& Error, bool bTerminal, bool bAuth)
{
	if (bEnded)
	{
		return;
	}
	const bool bWasEstablished = bEstablished;
	const bool bRetry = bAutomaticReconnect && bIsIdentitySet && !bTerminal &&
	                    !(bAuth && (!TokenProvider.IsBound() || bUsedFreshToken));
	bEstablished = false;
	bReconnecting = bRetry;
	bEnded = !bRetry;
	bAttemptPending = bRetry;
	bForceTokenRefresh |= bAuth;
	ResetTransport();
	const uint64 Generation = ConnectionGeneration;
	double Delay = 0;
	if (bRetry)
	{
		ReconnectAttempt = ReconnectAttempt < MAX_int32 ? ReconnectAttempt + 1 : MAX_int32;
		Delay = ReconnectOptions.Delay(ReconnectAttempt, FMath::FRand());
		ReconnectAt = FPlatformTime::Seconds() + Delay;
	}
	SuspendSubscriptions(!bRetry);
	ClearPendingOperations(TEXT("Connection lost before the result was received; execution outcome is unknown"));
	if (ConnectionGeneration != Generation)
	{
		return;
	} // A callback cancelled recovery.
	if (bWasEstablished)
	{
		OnDisconnectBaseDelegate.ExecuteIfBound(this, Error, bRetry, bRetry ? ReconnectAttempt : 0, float(Delay));
	}
	else
	{
		OnConnectErrorDelegate.ExecuteIfBound(Error, bRetry, bRetry ? ReconnectAttempt : 0, float(Delay));
	}
}

bool UDbConnectionBase::TokenNeedsRefresh() const
{
	TArray<FString> Parts;
	Token.ParseIntoArray(Parts, TEXT("."), false);
	if (Parts.Num() != 3)
	{
		return true;
	}
	FString Payload = Parts[1].Replace(TEXT("-"), TEXT("+")).Replace(TEXT("_"), TEXT("/"));
	while (Payload.Len() % 4)
	{
		Payload += TEXT("=");
	}
	FString Json;
	if (!FBase64::Decode(Payload, Json))
	{
		return true;
	}
	TSharedPtr<FJsonObject> Claims;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Json), Claims) || !Claims.IsValid())
	{
		return true;
	}
	double Expiry = 0, Issued = 0;
	if (!Claims->TryGetNumberField(TEXT("exp"), Expiry) || !FMath::IsFinite(Expiry))
	{
		return true;
	}
	const bool bHasIssued = Claims->TryGetNumberField(TEXT("iat"), Issued) && FMath::IsFinite(Issued);
	const double Margin = bHasIssued ? FMath::Max(30.0, (Expiry - Issued) * 0.05) : 30.0;
	return Expiry - double(FDateTime::UtcNow().ToUnixTimestamp()) <= Margin;
}

void UDbConnectionBase::AdvanceReconnect()
{
	if (bEnded || !bAttemptPending || FPlatformTime::Seconds() < ReconnectAt)
	{
		return;
	}
	bAttemptPending = false;
	bUsedFreshToken = false;
	if (TokenProvider.IsBound() && (bForceTokenRefresh || TokenNeedsRefresh()))
	{
		TokenRequest = NewObject<UReconnectTokenRequest>(this);
		TWeakObjectPtr<UDbConnectionBase> WeakThis(this);
		const uint64 Generation = ConnectionGeneration;
		TokenRequest->Completion = [WeakThis, Generation](const FString& NewToken, const FString& Error)
		{
			if (!WeakThis.IsValid() || WeakThis->ConnectionGeneration != Generation || WeakThis->bEnded)
			{
				return;
			}
			// Credential completion may open the socket; lifecycle callbacks run inside FrameTick.
			auto* Conn = WeakThis.Get();
			if (!Error.IsEmpty() || NewToken.IsEmpty())
			{
				Conn->QueueFailure(Error.IsEmpty() ? TEXT("Token provider returned an empty token") : Error, false);
				Conn->TokenRequest = nullptr;
				return;
			}
			Conn->Token = NewToken;
			Conn->bUsedFreshToken = true;
			Conn->bForceTokenRefresh = false;
			Conn->TokenRequest = nullptr;
			Conn->StartSocket();
		};
		TokenProvider.Execute(TokenRequest);
	}
	else
	{
		StartSocket();
	}
}

void UDbConnectionBase::EndSubscription(USubscriptionHandleBase* Handle)
{
	if (!Handle || Handle->bEnded)
	{
		return;
	}
	ActiveSubscriptions.Remove(Handle->QuerySetId);
	Handle->bActive = false;
	Handle->bEnded = true;
	FSubscriptionEventContextBase Context;
	Context.Event = FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit());
	Handle->EndDelegate.ExecuteIfBound(Context);
}

void UDbConnectionBase::SuspendSubscriptions(bool bTerminal)
{
	TArray<TObjectPtr<USubscriptionHandleBase>> Handles;
	ActiveSubscriptions.GenerateValueArray(Handles);
	for (auto Handle : Handles)
	{
		if (!Handle)
		{
			continue;
		}
		Handle->bActive = false;
		if (bTerminal || Handle->bUnsubscribeCalled)
		{
			EndSubscription(Handle);
		}
	}
}

void UDbConnectionBase::ReplaySubscriptions()
{
	FSubscribeBatchType Batch;
	Batch.RequestId = GetNextRequestId();
	TArray<TObjectPtr<USubscriptionHandleBase>> Handles;
	ActiveSubscriptions.GenerateValueArray(Handles);
	ActiveSubscriptions.Empty();
	for (auto Handle : Handles)
	{
		if (!Handle || Handle->bEnded || Handle->bUnsubscribeCalled)
		{
			continue;
		}
		Handle->QuerySetId = GetNextSubscriptionId();
		ActiveSubscriptions.Add(Handle->QuerySetId, Handle);
		FSubscribeSetType Set;
		Set.QuerySetId.Id = Handle->QuerySetId;
		Set.QueryStrings = Handle->QuerySqls;
		Batch.Sets.Add(Set);
		ReplayQueryIds.Add(Handle->QuerySetId);
	}
	ReplayRequestId = Batch.RequestId;
	bPreparingReplay = false;
	if (Batch.Sets.IsEmpty())
	{
		FSubscribeBatchAppliedType Applied;
		Applied.RequestId = Batch.RequestId;
		ApplyReplay(Applied);
	}
	else if (!SendRawMessage(UE::SpacetimeDB::Serialize(FClientMessageType::SubscribeBatch(Batch))))
	{
		QueueFailure(TEXT("Disconnected while replaying subscriptions"), false);
	}
}

void UDbConnectionBase::ApplyReplay(const FSubscribeBatchAppliedType& Batch)
{
	if (!ReplayRequestId || Batch.RequestId != ReplayRequestId || Batch.Results.Num() != ReplayQueryIds.Num())
	{
		HandleProtocolViolation(TEXT("Unexpected subscription replay response"));
		return;
	}
	for (int32 Index = 0; Index < Batch.Results.Num(); ++Index)
	{
		if (Batch.Results[Index].QuerySetId.Id != ReplayQueryIds[Index])
		{
			HandleProtocolViolation(TEXT("Mismatched replay query set"));
			return;
		}
	}
	ReplayRequestId = 0;
	ReplayQueryIds.Empty();
	FTransactionUpdateType Transaction;
	FQuerySetUpdateType Reconciliation;
	for (const auto& Pair : RegisteredTables)
	{
		FTableUpdateType Table;
		Table.TableName = Pair.Key;
		FPersistentTableRowsType Rows;
		Pair.Value->AppendCachedRows(Pair.Key, Rows.Deletes);
		Table.Rows.Add(FTableUpdateRowsType::PersistentTable(Rows));
		Reconciliation.Tables.Add(Table);
	}
	for (const auto& Result : Batch.Results)
	{
		if (Result.Outcome.IsApplied())
		{
			Reconciliation.Tables.Append(QueryRowsToDatabaseUpdate(Result.Outcome.GetAsApplied(), false).Tables);
		}
	}
	Transaction.QuerySets.Add(Reconciliation);
	const FDatabaseUpdateType Update = TransactionUpdateToDatabaseUpdate(Transaction);
	const uint64 Generation = ConnectionGeneration;
	PreProcessDatabaseUpdate(Update);
	// Apply every table before any row or subscription callback observes the cache.
	DbUpdate(Update, FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit()));
	for (const auto& Result : Batch.Results)
	{
		if (bEnded || ConnectionGeneration != Generation)
		{
			return;
		}
		auto* Found = ActiveSubscriptions.Find(Result.QuerySetId.Id);
		if (!Found || !*Found)
		{
			continue;
		}
		auto Handle = *Found;
		if (Result.Outcome.IsError())
		{
			FErrorContextBase Context;
			Context.Error = Result.Outcome.GetAsError();
			ActiveSubscriptions.Remove(Result.QuerySetId.Id);
			Handle->TriggerError(Context);
		}
		else
		{
			FSubscriptionEventContextBase Context;
			Context.Event = FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit());
			Handle->TriggerApplied(Context);
		}
	}
}

void UDbConnectionBase::AcceptIncomingEvent(uint64 Generation, uint64 Id, FIncomingEvent Event)
{
	if (!bEnded && Generation == ConnectionGeneration)
	{
		IncomingEvents.Add(Id, MoveTemp(Event));
	}
}
