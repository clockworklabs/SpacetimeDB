#include "Connection/DbConnectionBase.h"
#include "Connection/DbConnectionBuilder.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/Types/ClientMessageType.g.h"
#include "ModuleBindings/Types/SubscribeMultiType.g.h"
#include "ModuleBindings/Types/UnsubscribeMultiType.g.h"
#include "ModuleBindings/Types/SubscribeMultiAppliedType.g.h"
#include "ModuleBindings/Types/UnsubscribeMultiAppliedType.g.h"
#include "ModuleBindings/Types/SubscriptionErrorType.g.h"
#include "ModuleBindings/Types/DatabaseUpdateType.g.h"
#include "ModuleBindings/Types/CompressableQueryUpdateType.g.h"
#include "Misc/Compression.h"
#include "Misc/ScopeLock.h"
#include "Async/Async.h"
#include "BSATN/UEBSATNHelpers.h"

UDbConnectionBase::UDbConnectionBase(const FObjectInitializer& ObjectInitializer)
	: Super(ObjectInitializer)
{
	NextRequestId = 1;
	NextSubscriptionId = 1;
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

	UE_LOG(LogTemp, Warning, TEXT("TryGetIdentity called before identity was set"));
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
	if (OnConnectErrorDelegate.IsBound())
	{
		OnConnectErrorDelegate.Execute(Error);
	}
}

void UDbConnectionBase::HandleWSClosed(int32 /*StatusCode*/, const FString& Reason, bool /*bWasClean*/)
{
	if (OnDisconnectBaseDelegate.IsBound())
	{
		OnDisconnectBaseDelegate.Execute(this, Reason);
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
		FServerMessageType Parsed = This->PreProcessMessage(Message);

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
	bool bIsValid = false;
	switch (Message.Tag)
	{
	case EServerMessageTag::InitialSubscription:
	{
		//@Note: This is a legacy tag, used implemented in current server version
		break;
	}
	case EServerMessageTag::TransactionUpdate:
	{
		// Process a transaction update message
		const FTransactionUpdateType Payload = Message.GetAsTransactionUpdate();

		// Create a status object based on the transaction status
		FSpacetimeDBStatus StatusObj;
		bool bSuccess = false;
		FString ErrorMessage;
		if (Payload.Status.IsCommitted())
		{
			bSuccess = true;
			StatusObj = FSpacetimeDBStatus::Committed(FSpacetimeDBUnit());
		}
		else if (Payload.Status.IsFailed())
		{
			ErrorMessage = Payload.Status.GetAsFailed();
			StatusObj = FSpacetimeDBStatus::Failed(ErrorMessage);
		}
		else if (Payload.Status.IsOutOfEnergy())
		{
			Payload.Status.GetAsOutOfEnergy();
			StatusObj = FSpacetimeDBStatus::OutOfEnergy(FSpacetimeDBUnit());
			ErrorMessage = TEXT("Out of energy");
		}

		// Process the transaction update and create a reducer event
		FReducerEvent RedEvent;
		RedEvent.Timestamp = Payload.Timestamp;
		RedEvent.Status = StatusObj;
		RedEvent.CallerIdentity = Payload.CallerIdentity;
		RedEvent.CallerConnectionId = Payload.CallerConnectionId;
		RedEvent.EnergyConsumed = Payload.EnergyQuantaUsed;
		RedEvent.ReducerCall = Payload.ReducerCall;

		// If the status is committed, we update the database
		if (bSuccess)
		{
			DbUpdate(Payload.Status.GetAsCommitted(), FSpacetimeDBEvent::Reducer(RedEvent)); // Update table and trigger insert/update/delete
			ReducerEvent(RedEvent); // Trigger the reducer event
		}
		else
		{
			ReducerEvent(RedEvent); // Trigger the reducer event
			ReducerEventFailed(RedEvent, ErrorMessage);
		}
		break;
	}
	case EServerMessageTag::TransactionUpdateLight:
	{
		// Process a light transaction update message
		const FTransactionUpdateLightType Payload = Message.GetAsTransactionUpdateLight();

		//@TODO: Implement light update fully
		DbUpdate(Payload.Update, FSpacetimeDBEvent::UnknownTransaction(FSpacetimeDBUnit()));

		break;
	}
	case EServerMessageTag::IdentityToken:
	{
		// Process an identity token message
		const FIdentityTokenType Payload = Message.GetAsIdentityToken();

		Token = Payload.Token;
		UCredentials::SaveToken(Token);
		Identity = Payload.Identity;
		bIsIdentitySet = true;
		UE_LOG(LogTemp, Verbose, TEXT("IdentityToken: Identity set to: %s"), *Identity.ToHex());
		ConnectionId = Payload.ConnectionId;
		if (OnConnectBaseDelegate.IsBound())
		{
			OnConnectBaseDelegate.Execute(this, Identity, Token);
		}
		break;
	}
	case EServerMessageTag::OneOffQueryResponse:
	{
		//@Note: Not implemented in Rust version, skip for now here aswell
		break;
	}
	case EServerMessageTag::SubscribeApplied:
	{
		//@Note: This is a legacy tag, not implemented in current server version
		break;
	}
	case EServerMessageTag::UnsubscribeApplied:
	{
		//@Note: This is a legacy tag, not implemented in current server version
		break;
	}
	case EServerMessageTag::SubscriptionError:
	{
		// Process a subscription error message
		const FSubscriptionErrorType Payload = Message.GetAsSubscriptionError();
		if (TObjectPtr<USubscriptionHandleBase> Handle = *ActiveSubscriptions.Find(Payload.QueryId.Value))
		{
			if (!Handle)
			{
				UE_LOG(LogTemp, Error, TEXT("SubscriptionError: Null handle for QueryId %u. Error: %s"),
					Payload.QueryId.Value,
					*Payload.Error);
				return;
			}
			FErrorContextBase Ctx; Ctx.Error = Payload.Error;
			Handle->TriggerError(Ctx);
			ActiveSubscriptions.Remove(Payload.QueryId.Value);
		}
		break;
	}
	case EServerMessageTag::SubscribeMultiApplied:
	{
		// Process a multi-subscription applied message
		const FSubscribeMultiAppliedType Payload = Message.GetAsSubscribeMultiApplied();
		// Update the database with the subscription applied event
		DbUpdate(Payload.Update, FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit()));

		if (TObjectPtr<USubscriptionHandleBase> Handle = *ActiveSubscriptions.Find(Payload.QueryId.Id))
		{
			if (!Handle)
			{
				UE_LOG(LogTemp, Error, TEXT("SubscriptionError: Null handle for QueryId %u."), Payload.QueryId.Id);
				return;
			}
			FSubscriptionEventContextBase Ctx; Ctx.Event = FSpacetimeDBEvent::SubscribeApplied(FSpacetimeDBUnit());
			Handle->TriggerApplied(Ctx);
		}

		break;
	}
	case EServerMessageTag::UnsubscribeMultiApplied:
	{
		// Process a multi-unsubscription applied message
		const FUnsubscribeMultiAppliedType Payload = Message.GetAsUnsubscribeMultiApplied();

		// Update the database with the unsubscription applied event
		DbUpdate(Payload.Update, FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit()));
		if (TObjectPtr<USubscriptionHandleBase> Handle = *ActiveSubscriptions.Find(Payload.QueryId.Id))
		{
			if (!Handle)
			{
				UE_LOG(LogTemp, Error, TEXT("UnsubscribeMultiApplied: Null handle for QueryId %u."), Payload.QueryId.Id);
				return;
			}
			Handle->bEnded = true;
			Handle->bActive = false;
			Handle->bUnsubscribeCalled = true;
			FSubscriptionEventContextBase Ctx; Ctx.Event = FSpacetimeDBEvent::UnsubscribeApplied(FSpacetimeDBUnit());
			if (Handle->EndDelegate.IsBound())
			{
				Handle->EndDelegate.Execute(Ctx);
			}
			ActiveSubscriptions.Remove(Payload.QueryId.Id);
		}
		break;
	}
	default:
		// Unknown tag - bail out
		UE_LOG(LogTemp, Warning, TEXT("Unknown server-message tag"));
		break;
	}
}

bool UDbConnectionBase::DecompressBrotli(const TArray<uint8>& InData, TArray<uint8>& OutData)
{
	UE_LOG(LogTemp, Error, TEXT("Brotli decompression unavilable"));
	return false;
}

bool UDbConnectionBase::DecompressGzip(const TArray<uint8>& InData, TArray<uint8>& OutData)
{
	if (InData.Num() < 4)
	{
		UE_LOG(LogTemp, Error, TEXT("Gzip data too small"));
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
		UE_LOG(LogTemp, Error, TEXT("Gzip decompression failed"));
		return false;
	}

	OutData.SetNum(OutSize);
	return true;
}

bool UDbConnectionBase::DecompressPayload(ECompressableQueryUpdateTag Variant, const TArray<uint8>& In, TArray<uint8>& Out)
{
	switch (Variant)
	{
	case ECompressableQueryUpdateTag::Uncompressed:
		// No compression, just copy the data
		Out = In;
		return true;
	case ECompressableQueryUpdateTag::Brotli:
		return DecompressBrotli(In, Out);
	case ECompressableQueryUpdateTag::Gzip:
		return DecompressGzip(In, Out);
	default:
		UE_LOG(LogTemp, Error, TEXT("Unknown compression variant"));
		return false;
	}
}

void UDbConnectionBase::PreProcessDatabaseUpdate(const FDatabaseUpdateType& Update)
{
	for (const FTableUpdateType& TableUpdate : Update.Tables)
	{
		TArray<FCompressableQueryUpdateType> UncompressedCQUs;
		for (const FCompressableQueryUpdateType& CQU : TableUpdate.Updates)
		{
	
			// Uncompress the CQU based on its tag
			FQueryUpdateType UncompressedUpdate;
			switch (CQU.Tag)
			{
			case ECompressableQueryUpdateTag::Uncompressed:
				UncompressedUpdate = CQU.GetAsUncompressed();
				break;
			case ECompressableQueryUpdateTag::Brotli:
			{
				TArray<uint8> Data = CQU.GetAsBrotli();
				TArray<uint8> Dec;
				if (DecompressBrotli(Data, Dec))
				{
					//@Note: This will never trigger until Brotli decompression is implemented
					UncompressedUpdate = UE::SpacetimeDB::Deserialize<FQueryUpdateType>(Dec);
				}
				break;
			}
			case ECompressableQueryUpdateTag::Gzip:
			{
				TArray<uint8> Data = CQU.GetAsGzip();
				TArray<uint8> Dec;
				if (DecompressGzip(Data, Dec))
				{
					UncompressedUpdate = UE::SpacetimeDB::Deserialize<FQueryUpdateType>(Dec);
				}
				break;
			}
			default:
				UE_LOG(LogTemp, Error, TEXT("Unknown compression variant in CQU"));
				break;
			}
			UncompressedCQUs.Add(FCompressableQueryUpdateType::Uncompressed(UncompressedUpdate));
			UE_LOG(LogTemp, Verbose, TEXT("Table %s Inserts:%d Deletes:%d"), *TableUpdate.TableName, UncompressedUpdate.Inserts.RowsData.Num(), UncompressedUpdate.Deletes.RowsData.Num());
		}

		// After ensuring all updates are uncompressed, attempt to deserialize rows
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
				UE_LOG(LogTemp, Error, TEXT("No deserializer found for table %s"), *TableUpdate.TableName);
			}
		}
		if (Deserializer)
		{
			// Preprocess the table data using the deserializer
			TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase> Data = Deserializer->PreProcess(UncompressedCQUs, TableUpdate.TableName);
			if (Data.IsValid())
			{
				// Store the preprocessed data in the mutex-protected map
				FScopeLock Lock(&PreprocessedDataMutex);
				FPreprocessedTableKey Key(TableUpdate.TableId, TableUpdate.TableName);
				TArray<TSharedPtr<UE::SpacetimeDB::FPreprocessedTableDataBase>>& Queue = PreprocessedTableData.FindOrAdd(Key);
				Queue.Add(Data);
			}
		}
		else
		{
			UE_LOG(LogTemp, Error, TEXT("Skipping table %s updates due to missing deserializer"), *TableUpdate.TableName);
		}
	}
}

FServerMessageType UDbConnectionBase::PreProcessMessage(const TArray<uint8>& Message)
{
	if (Message.Num() == 0)
	{
		UE_LOG(LogTemp, Error, TEXT("Empty message recived from server, ignored"));
		return FServerMessageType{};
	}
	// Check if the first byte is a valid compression tag
	ECompressableQueryUpdateTag Compression = static_cast<ECompressableQueryUpdateTag>(Message[0]);
	TArray<uint8> CompressedPayload;
	CompressedPayload.Append(Message.GetData() + 1, Message.Num() - 1);

	// Decompress the payload based on the compression tag
	TArray<uint8> Decompressed;
	if (!DecompressPayload(Compression, CompressedPayload, Decompressed))
	{
		UE_LOG(LogTemp, Error, TEXT("Failed to decompress incoming message"));
		return FServerMessageType{};
	}

	// Deserialize the decompressed data into a UServerMessageType object
	FServerMessageType Parsed = UE::SpacetimeDB::Deserialize<FServerMessageType>(Decompressed);

	// Process it based on its tag. Messages containing rows will be deserialized into rows based on registered type and table name.
	bool bValid = false;
	switch (Parsed.Tag)
	{
		case EServerMessageTag::InitialSubscription:
		{
			const FInitialSubscriptionType Payload = Parsed.GetAsInitialSubscription();
			// PreProcess the initial subscription payload
			PreProcessDatabaseUpdate(Payload.DatabaseUpdate);
			break;
		}
		case EServerMessageTag::TransactionUpdate:
		{

			const FTransactionUpdateType Payload = Parsed.GetAsTransactionUpdate();
			if (Payload.Status.IsCommitted())
			{
				// PreProcess the database update with the committed status
				PreProcessDatabaseUpdate(Payload.Status.GetAsCommitted());
			}
			break;
		}
		case EServerMessageTag::TransactionUpdateLight:
		{
			//@Note: Light tag in not implemented as an option in connection builder, this will never trigger but we keep this for future compatibility
			const FTransactionUpdateLightType Payload = Parsed.GetAsTransactionUpdateLight();
			// PreProcess the light transaction update
			PreProcessDatabaseUpdate(Payload.Update);
			break;
		}
		case EServerMessageTag::SubscribeMultiApplied:
		{
			const FSubscribeMultiAppliedType Payload = Parsed.GetAsSubscribeMultiApplied();
			PreProcessDatabaseUpdate(Payload.Update);
			break;
		}
		case EServerMessageTag::UnsubscribeMultiApplied:
		{
			const FUnsubscribeMultiAppliedType Payload = Parsed.GetAsUnsubscribeMultiApplied();
			PreProcessDatabaseUpdate(Payload.Update);
			break;
		}
		default:
			break;
	}
	return Parsed;
}


int32 UDbConnectionBase::GetNextRequestId()
{
	return NextRequestId++;
}

int32 UDbConnectionBase::GetNextSubscriptionId()
{
	return NextSubscriptionId++;
}

void UDbConnectionBase::StartSubscription(USubscriptionHandleBase* Handle)
{
	if (!Handle)
	{
		UE_LOG(LogTemp, Error, TEXT("StartSubscription called with null handle"));
		return;
	}

	if (Handle->QuerySqls.Num() == 0)
	{
		UE_LOG(LogTemp, Error, TEXT("StartSubscription called with empty query list"));
		return;
	}
	
	const int32 QueryId = GetNextSubscriptionId();
	Handle->QueryId = QueryId;
	Handle->ConnInternal = this;
	ActiveSubscriptions.Add(QueryId, Handle);

	FSubscribeMultiType SubMsg;
	SubMsg.QueryStrings = Handle->QuerySqls;
	SubMsg.RequestId = GetNextRequestId();
	SubMsg.QueryId.Id = QueryId;

	FClientMessageType Msg = FClientMessageType::SubscribeMulti(SubMsg);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
}

void UDbConnectionBase::UnsubscribeInternal(USubscriptionHandleBase* Handle)
{
	if (!Handle || Handle->bEnded)
	{
		return;
	}

	const int32 QueryId = Handle->QueryId;
	FUnsubscribeMultiType MsgData;
	MsgData.RequestId = GetNextRequestId();
	MsgData.QueryId.Id = QueryId;

	FClientMessageType Msg = FClientMessageType::UnsubscribeMulti(MsgData);
	TArray<uint8> Data = UE::SpacetimeDB::Serialize(Msg);
	SendRawMessage(Data);
}

void UDbConnectionBase::InternalCallReducer(const FString& Reducer, TArray<uint8> Args, USetReducerFlagsBase* Flags)
{

	if (!WebSocket || !WebSocket->IsConnected())
	{
		UE_LOG(LogTemp, Error, TEXT("Cannot call reducer, not connected to server!"));
		return;
	}

	uint8 FlagToUse = 0; // Default to FullUpdate
	if (Flags && Flags->FlagMap.Contains(Reducer))
	{
		//Select flag if set by user
		ECallReducerFlags FlagFound = *Flags->FlagMap.Find(Reducer);
		FlagToUse = static_cast<uint8>(FlagFound);
	}

	FCallReducerType MsgData;
	MsgData.Reducer = Reducer;
	MsgData.Args = Args;
	MsgData.RequestId = GetNextRequestId();
	MsgData.Flags = FlagToUse;

	FClientMessageType Msg = FClientMessageType::CallReducer(MsgData);
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