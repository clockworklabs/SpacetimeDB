#include "Connection/DbConnectionBase.h"
#include "Connection/DbConnectionBuilder.h"
#include "Connection/Credentials.h"
#include "Connection/LogCategory.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "Misc/Compression.h"
#include "Misc/ScopeLock.h"
#include "Async/Async.h"
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
}

UDbConnectionBase::UDbConnectionBase(const FObjectInitializer& ObjectInitializer)
	: Super(ObjectInitializer)
{
	NextRequestId = 1;
	NextSubscriptionId = 1;
	ProcedureCallbacks = CreateDefaultSubobject<UProcedureCallbacks>(TEXT("ProcedureCallbacks"));
}

void UDbConnectionBase::Disconnect()
{
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
	bProtocolViolationHandled = false;
	ClearPendingOperations(Error);
	if (OnConnectErrorDelegate.IsBound())
	{
		OnConnectErrorDelegate.Execute(Error);
	}
}

void UDbConnectionBase::HandleWSClosed(int32 /*StatusCode*/, const FString& Reason, bool /*bWasClean*/)
{
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

void UDbConnectionBase::HandleWSBinaryMessage(const TArray<uint8>& Message)
{
	//tag for arrival order
	const int32 Id = NextPreprocessId.GetValue();
	NextPreprocessId.Increment();

	//do expensive work off-thread
	TWeakObjectPtr<UDbConnectionBase> WeakThis(this);
	Async(EAsyncExecution::Thread, [WeakThis, Message, Id]()
	{
		if (!WeakThis.IsValid())
		{
			return;
		}
		UDbConnectionBase* This = WeakThis.Get();

		//parse the message, decompress if needed
		FServerMessageType Parsed;
		if (!This->PreProcessMessage(Message, Parsed))
		{
			AsyncTask(ENamedThreads::GameThread, [WeakThis]()
			{
				if (!WeakThis.IsValid())
				{
					return;
				}
				UDbConnectionBase* Conn = WeakThis.Get();
				Conn->HandleProtocolViolation(TEXT("Failed to parse/decompress incoming WebSocket message"));
			});
			return;
		}

		//queue: re-order buffer
		TArray<FServerMessageType> Ready;
		{
			FScopeLock Lock(&This->PreprocessMutex);
			// Move the parsed message into the map to avoid copying
			This->PreprocessedMessages.Add(Id, MoveTemp(Parsed));
			//check if we can release any messages in order
			while (This->PreprocessedMessages.Contains(This->NextReleaseId))
			{
				Ready.Add(This->PreprocessedMessages.FindAndRemoveChecked(This->NextReleaseId));
				++This->NextReleaseId;
			}
		}
		//if we have any ready messages, append them to the pending messages list that is processed in Tick
		if (Ready.Num() > 0)
		{
			FScopeLock Lock(&This->PendingMessagesMutex);
			This->PendingMessages.Append(MoveTemp(Ready));
		}
	});
}

void UDbConnectionBase::FrameTick()
{
	TArray<FServerMessageType> Local;
	{
		FScopeLock Lock(&PendingMessagesMutex);
		if (PendingMessages.Num() == 0)
		{
			//nothing to process, return early
			return;
		}
		//move pending messages to local array for processing
		Local = MoveTemp(PendingMessages);
		PendingMessages.Empty();
	}

	//process all messages in the local array
	for (const FServerMessageType& Msg : Local)
	{
		//process the message, this will call DbUpdate or trigger subscription events as needed
		ProcessServerMessage(Msg);
	}
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


void UDbConnectionBase::ProcessServerMessage(const FServerMessageType& Message)
{
	switch (Message.Tag)
	{
	case EServerMessageTag::InitialConnection:
	{
		const FInitialConnectionType Payload = Message.GetAsInitialConnection();
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
		const FSubscriptionErrorType Payload = Message.GetAsSubscriptionError();
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
		const FReducerResultType Payload = Message.GetAsReducerResult();
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

void UDbConnectionBase::PreProcessDatabaseUpdate(const FDatabaseUpdateType& Update)
{
	for (const FTableUpdateType& TableUpdate : Update.Tables)
	{
		// Attempt to deserialize rows after payload decode.
		TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer> Deserializer;
		{
			// Find the deserializer for this table
			FScopeLock Lock(&TableDeserializersMutex);
			if (TSharedPtr<UE::SpacetimeDB::ITableRowDeserializer>* Found = TableDeserializers.Find(TableUpdate.TableName))
			{
				// If found, use the deserializer
				Deserializer = *Found;
			}
			else
			{
				UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("No deserializer found for table %s"), *TableUpdate.TableName);
			}
		}
		if (Deserializer)
		{
			TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase> Data = Deserializer->PreProcess(TableUpdate.Rows, TableUpdate.TableName);
			if (Data.IsValid())
			{
				FScopeLock Lock(&PreprocessedDataMutex);
				FPreprocessedTableKey Key(TableUpdate.TableName);
				TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>& Queue = PreprocessedTableData.FindOrAdd(Key);
				Queue.Add(Data);
			}
		}
		else
		{
			UE_LOG(LogSpacetimeDb_Connection, Error, TEXT("Skipping table %s updates due to missing deserializer"), *TableUpdate.TableName);
		}
	}
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

	// Preprocess row-bearing payloads for table deserializers.
	switch (OutMessage.Tag)
	{
		case EServerMessageTag::SubscribeApplied:
		{
			const FSubscribeAppliedType Payload = OutMessage.GetAsSubscribeApplied();
			PreProcessDatabaseUpdate(QueryRowsToDatabaseUpdate(Payload.Rows, false));
			break;
		}
		case EServerMessageTag::UnsubscribeApplied:
		{
			const FUnsubscribeAppliedType Payload = OutMessage.GetAsUnsubscribeApplied();
			if (Payload.Rows.IsSet())
			{
				PreProcessDatabaseUpdate(QueryRowsToDatabaseUpdate(Payload.Rows.Value, true));
			}
			break;
		}
		case EServerMessageTag::TransactionUpdate:
		{
			const FTransactionUpdateType Payload = OutMessage.GetAsTransactionUpdate();
			PreProcessDatabaseUpdate(TransactionUpdateToDatabaseUpdate(Payload));
			break;
		}
		case EServerMessageTag::ReducerResult:
		{
			const FReducerResultType Payload = OutMessage.GetAsReducerResult();
			if (Payload.Result.IsOk())
			{
				PreProcessDatabaseUpdate(TransactionUpdateToDatabaseUpdate(Payload.Result.GetAsOk().TransactionUpdate));
			}
			break;
		}
		default:
			break;
	}
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
