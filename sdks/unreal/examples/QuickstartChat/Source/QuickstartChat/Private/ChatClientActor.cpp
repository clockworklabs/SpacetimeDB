#include "ChatClientActor.h"
#include "Connection/Credentials.h"
#include "Connection/DbConnectionBuilder.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"


AChatClientActor::AChatClientActor()
{
	PrimaryActorTick.bCanEverTick = true;
	Conn = nullptr;
	LocalIdentity = FSpacetimeDBIdentity();
}



void AChatClientActor::BeginPlay()
{
	Super::BeginPlay();

	// Abort initialization if the actor is inactive
	if (!bActive)
	{
		return;
	}

	// Connection details for the local SpacetimeDB instance
	// @note: Make sure the SpacetimeDB server is running. Replace the host and database name with your server details if need be.
	Host = TEXT("127.0.0.1:3000"); // SpacetimeDB server address (default is localhost:3000)
	DbName = TEXT("quickstart-chat"); // module name used by the sample server

	// Load any previously saved authentication token
	UCredentials::Init(TEXT(".spacetime_unreal_quickstart")); //Can be any path you want, it will be used to save the token
	const FString SavedToken = UCredentials::LoadToken();

	// Setup delegate handlers for connection lifecycle events
	FOnConnectDelegate ConnectDelegate;
	BIND_DELEGATE_SAFE(ConnectDelegate, this, AChatClientActor, OnConnected);

	FOnDisconnectDelegate DisconnectDelegate;
	BIND_DELEGATE_SAFE(DisconnectDelegate, this, AChatClientActor, OnDisconnected);

	FOnConnectErrorDelegate ErrorDelegate;
	BIND_DELEGATE_SAFE(ErrorDelegate, this, AChatClientActor, OnConnectError);

	// Build the connection using the fluent builder API
	Conn = UDbConnection::Builder()
		->WithUri(Host)                // Host address to connect to
		->WithModuleName(DbName)        // Database/module name
		->WithToken(SavedToken)         // Optional authentication token
		->WithCompression(ESpacetimeDBCompression::Gzip) // Enable gzip compression
		->OnConnect(ConnectDelegate)    // Bind connect handler
		->OnDisconnect(DisconnectDelegate) // Bind disconnect handler
		->OnConnectError(ErrorDelegate) // Bind failure handler
		->Build();

	// Register table and reducer callbacks after connection creation
	RegisterCallbacks();
}

void AChatClientActor::EndPlay(const EEndPlayReason::Type EndPlayReason)
{
	// Close the connection when the actor is removed
	if (Conn)
	{
		Conn->Disconnect();
	}

	Super::EndPlay(EndPlayReason);
}

void AChatClientActor::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
}

void AChatClientActor::RegisterCallbacks()
{
	// Listen for changes on the user table
	Conn->Db->User->OnInsert.AddDynamic(this, &AChatClientActor::OnUserInsert);
	Conn->Db->User->OnDelete.AddDynamic(this, &AChatClientActor::OnUserDelete);
	Conn->Db->User->OnUpdate.AddDynamic(this, &AChatClientActor::OnUserUpdate);

	// Listen for changes on the message table
	Conn->Db->Message->OnInsert.AddDynamic(this, &AChatClientActor::OnMessageInsert);
	Conn->Db->Message->OnDelete.AddDynamic(this, &AChatClientActor::OnMessageDelete);
	Conn->Db->Message->OnUpdate.AddDynamic(this, &AChatClientActor::OnMessageUpdate);

	// Example avlilable unbind usage:
	// Conn->Db->Message->OnDelete.RemoveAll(this);
	// UNBIND_DELEGATE_SAFE(Conn->Db->Message->OnDelete, this, AChatClientActor, OnMessageDelete);

	// Opt in to receive the reducer result and any table updates
	Conn->SetReducerFlags->SendMessage(ECallReducerFlags::FullUpdate);
	Conn->Reducers->OnSendMessage.AddDynamic(this, &AChatClientActor::OnReducerOnSendMessage);

	Conn->SetReducerFlags->SetName(ECallReducerFlags::FullUpdate);
	Conn->Reducers->OnSetName.AddDynamic(this, &AChatClientActor::OnReducerOnSetName);

	// Hook error delegate for any reducers without explicit bindings
	Conn->Reducers->InternalOnUnhandledReducerError.AddDynamic(this, &AChatClientActor::OnUnhandledReducerError);
}

void AChatClientActor::OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token)
{
	LocalIdentity = Identity;
	LogAndDisplayMessage(TEXT("Connected to SpacetimeDB"), FColor::Emerald);

	UCredentials::SaveToken(Token);

	SubscribeToAll();
}

void AChatClientActor::OnDisconnected(UDbConnection* Connection, const FString& ErrorMessage)
{
	const FString Message = FString::Printf(
		TEXT("OnDisconnected -> Error: %s"),
		ErrorMessage.IsEmpty() ? TEXT("None") : *ErrorMessage);
	LogAndDisplayMessage(Message, FColor::Red);
}

void AChatClientActor::OnConnectError(const FString& ErrorMessage)
{
	const FString Message = FString::Printf(
		TEXT("OnConnectError -> Error: %s "),
		*ErrorMessage);
	LogAndDisplayMessage(Message, FColor::Red);
}

void AChatClientActor::SubscribeToAll()
{

	// Bind subscription delegates
	FOnSubscriptionApplied SubscriptionApplyDelegate;
	BIND_DELEGATE_SAFE(SubscriptionApplyDelegate, this, AChatClientActor, OnSubscriptionApplied);

	FOnSubscriptionError SubscriptionErrorDelegate;
	BIND_DELEGATE_SAFE(SubscriptionErrorDelegate, this, AChatClientActor, OnSubscriptionError);

	// Subscribe to every table in the schema
	SubscriptionHandleAll = Conn->SubscriptionBuilder()
		->OnApplied(SubscriptionApplyDelegate)
		->OnError(SubscriptionErrorDelegate)
		->SubscribeToAllTables();
}

void AChatClientActor::SubscribeToUser()
{

	// Subscribe specifically to the user table
	FOnSubscriptionApplied SubscriptionApplyDelegate;
	BIND_DELEGATE_SAFE(SubscriptionApplyDelegate, this, AChatClientActor, OnSubscriptionApplied);

	FOnSubscriptionError SubscriptionErrorDelegate;
	BIND_DELEGATE_SAFE(SubscriptionErrorDelegate, this, AChatClientActor, OnSubscriptionError);

	SubscriptionHandleUser = Conn->SubscriptionBuilder()
		->OnApplied(SubscriptionApplyDelegate)
		->OnError(SubscriptionErrorDelegate)
		->Subscribe({ "SELECT * FROM user" });
}

void AChatClientActor::SubscribeToMessage()
{

	// Subscribe specifically to the message table
	FOnSubscriptionApplied SubscriptionApplyDelegate;
	BIND_DELEGATE_SAFE(SubscriptionApplyDelegate, this, AChatClientActor, OnSubscriptionApplied);

	FOnSubscriptionError SubscriptionErrorDelegate;
	BIND_DELEGATE_SAFE(SubscriptionErrorDelegate, this, AChatClientActor, OnSubscriptionError);

	SubscriptionHandleMessage = Conn->SubscriptionBuilder()
		->OnApplied(SubscriptionApplyDelegate)
		->OnError(SubscriptionErrorDelegate)
		->Subscribe({ "SELECT * FROM message" });
}


void AChatClientActor::UnsubscribeFromAll()
{
	if (!SubscriptionHandleAll) return;
	// Stop receiving updates from all tables
	SubscriptionHandleAll->Unsubscribe();
}

void AChatClientActor::UnsubscribeFromUser()
{
	if (!SubscriptionHandleUser) return;
	// Stop receiving updates from the user table
	SubscriptionHandleUser->Unsubscribe();
}

void AChatClientActor::UnsubscribeFromMessage()
{
	if (!SubscriptionHandleMessage) return;
	// Stop receiving updates from the message table
	SubscriptionHandleMessage->Unsubscribe();
}


void AChatClientActor::OnUserInsert(const FEventContext& Context, const FUserType& NewRow)
{
	if (NewRow.Online)
	{
		const FString Msg = FString::Printf(TEXT("%s is online"), *UserNameOrIdentity(NewRow));
		LogAndDisplayMessage(Msg, FColor::Green);
	}
}

void AChatClientActor::OnUserUpdate(const FEventContext& Context, const FUserType& OldRow, const FUserType& NewRow)
{
	if (OldRow.Name != NewRow.Name)
	{
		const FString Msg = FString::Printf(TEXT("%s renamed to %s"), *UserNameOrIdentity(OldRow), *NewRow.Name.Value);
		LogAndDisplayMessage(Msg, FColor::Yellow);
	}
	if (OldRow.Online != NewRow.Online)
	{
		if (NewRow.Online)
		{
			const FString Msg = FString::Printf(TEXT("%s connected."), *UserNameOrIdentity(NewRow));
			LogAndDisplayMessage(Msg, FColor::Emerald);
		}
		else
		{
			const FString Msg = FString::Printf(TEXT("%s disconnected."), *UserNameOrIdentity(NewRow));
			LogAndDisplayMessage(Msg, FColor::Orange);
		}
	}
}

void AChatClientActor::OnUserDelete(const FEventContext& Context, const FUserType& RemovedRow)
{
	// Inform about the deleted user record
	const FString HexId = RemovedRow.Identity.ToHex();
	const FString Message = FString::Printf(
		TEXT("OnUserDelete -> Identity: %s | Name: %s | Online: %s"),
		*HexId,
		RemovedRow.Name.bHasValue ? *RemovedRow.Name.Value : TEXT("None"),
		RemovedRow.Online ? TEXT("true") : TEXT("false"));

	LogAndDisplayMessage(Message, FColor::Red);
}

void AChatClientActor::OnMessageInsert(const FEventContext& Context, const FMessageType& NewRow)
{
	// Log a new inserted chat message
	if (!Context.Event.IsSubscribeApplied())
	{
		PrintMessage(NewRow);
	}
}

void AChatClientActor::OnMessageUpdate(const FEventContext& Context, const FMessageType& OldRow, const FMessageType& NewRow)
{
	// Display both the old and new values for the modified message
	const FString HexSender = OldRow.Sender.ToHex();
	const FString Message = FString::Printf(
		TEXT("OnMessageUpdate -> Sender: %s\nOld Timestamp: %s | Old Text: %s\nNew Timestamp: %s | New Text: %s"),
		*HexSender,
		*OldRow.Sent.ToString(),
		*OldRow.Text,
		*NewRow.Sent.ToString(),
		*NewRow.Text);

	LogAndDisplayMessage(Message, FColor::Yellow);
}

void AChatClientActor::OnMessageDelete(const FEventContext& Context, const FMessageType& DeletedRow)
{
	// Inform the user about the removed message
	const FString HexSender = DeletedRow.Sender.ToHex();
	const FString TimestampStr = DeletedRow.Sent.ToString();
	const FString Message = FString::Printf(
		TEXT("OnMessageDelete -> Sender: %s | Timestamp: %s | Text: %s"),
		*HexSender,
		*TimestampStr,
		*DeletedRow.Text);

	LogAndDisplayMessage(Message, FColor::Red);
}

void AChatClientActor::OnUnhandledReducerError(const FReducerEventContext& Context, const FString& Error)
{
	// Generic error handler for reducers without custom delegates
	const FString Message = FString::Printf(
		TEXT("OnUnhandledReducerError -> Error: %s"),
		*Error);

	LogAndDisplayMessage(Message, FColor::Red);
}


void AChatClientActor::OnReducerOnSetName(const FReducerEventContext& Context, const FString& Name)
{
	// Display the resulting name after the reducer call
	FString Message = FString::Printf(
		TEXT("OnReducerOnSetName -> Name: %s"),
		*Name);

	LogAndDisplayMessage(Message, FColor::Purple);
}

void AChatClientActor::OnReducerOnSendMessage(const FReducerEventContext& Context, const FString& Text)
{
	// Check context event if valid
	FString Message = FString::Printf(
		TEXT("OnReducerOnSendMessage -> Text: %s"),
		*Text);

	LogAndDisplayMessage(Message, FColor::Purple);
}


FString AChatClientActor::UserNameOrIdentity(const FUserType& User) const
{
	if (User.Name.bHasValue)
	{
		return User.Name.Value;
	}
	return User.Identity.ToHex().Left(8);
}

void AChatClientActor::PrintMessage(const FMessageType& Message)
{
	FUserType Sender = Conn->Db->User->Identity->Find(Message.Sender);
	FString SenderName = TEXT("unknown");
	if (Sender.Identity != FSpacetimeDBIdentity() || Sender.Name.bHasValue)
	{
		SenderName = UserNameOrIdentity(Sender);
	}

	const FString Output = FString::Printf(TEXT("%s: %s"), *SenderName, *Message.Text);
	LogAndDisplayMessage(Output, FColor::Silver);
}

void AChatClientActor::PrintMessagesInOrder()
{
	if (!Conn)
	{
		return;
	}

	TArray<FMessageType> Messages = Conn->Db->Message->Iter();
	Messages.Sort([](const FMessageType& A, const FMessageType& B)
		{
			return A.Sent < B.Sent;
		});

	for (const FMessageType& Msg : Messages)
	{
		PrintMessage(Msg);
	}
}

void AChatClientActor::OnSubscriptionApplied(FSubscriptionEventContext& Ctx)
{
	const FString Message = FString::Printf(
		TEXT("OnSubscriptionApplied -> Subscription applied "));
	LogAndDisplayMessage(Message, FColor::Emerald);
	PrintMessagesInOrder();
}

void AChatClientActor::OnSubscriptionError(const FErrorContext& Ctx)
{
	const FString Message = FString::Printf(
		TEXT("OnSubscriptionError -> Error: %s "),
		*Ctx.Error);
	LogAndDisplayMessage(Message, FColor::Red);
}


void AChatClientActor::PrintCurrentUser()
{
	// Verify we have a valid connection
	if (!Conn)
	{
		LogAndDisplayMessage(TEXT("Connection is not established."), FColor::Red);
		return;
	}

	// Fetch our identity from the connection
	FSpacetimeDBIdentity CurrentIdentity;
	if (!Conn->TryGetIdentity(CurrentIdentity))
	{
		LogAndDisplayMessage(TEXT("Failed to get current identity."), FColor::Red);
		return;
	}

	// Echo the identity to the log
	const FString CurrentHexId = "Current Identity found: " + CurrentIdentity.ToHex();
	LogAndDisplayMessage(CurrentHexId, FColor::Blue);

	// Retrieve the corresponding user row
	FUserType CurrentUser = Conn->Db->User->Identity->Find(CurrentIdentity);

	// Format a display string for the user
	const FString HexId = CurrentUser.Identity.ToHex();
	const FString Message = FString::Printf(
		TEXT("PrintCurrentUser -> Identity: %s | Name: %s | Online: %s"),
		*HexId,
		CurrentUser.Name.bHasValue ? *CurrentUser.Name.Value : TEXT("None"),
		CurrentUser.Online ? TEXT("true") : TEXT("false"));

	LogAndDisplayMessage(Message, FColor::Cyan);
}

void AChatClientActor::ReducerSetName(const FString NewName)
{
	if (!Conn)
	{
		UE_LOG(LogTemp, Warning, TEXT("Connection is not established."));
		return;
	}
	// Call the reducer to set the name
	Conn->Reducers->SetName(NewName);
}

void AChatClientActor::ReducerSendMessage(const FString Text)
{
	if (!Conn) 
	{
		UE_LOG(LogTemp, Warning, TEXT("Connection is not established."));
		return;
	}
	// Call the reducer to send a message
	Conn->Reducers->SendMessage(Text);
}

void AChatClientActor::ReducerSetRandomName()
{
	// Generate a random name for the user, used as call in editor function
	ReducerSetName(FString::Printf(TEXT("UEClient_%d"), FMath::RandRange(1, 1000)));
}

void AChatClientActor::ReducerSendRandomMessage()
{
	// Generate a random message, used as call in editor function
	ReducerSendMessage(FString::Printf(TEXT("Hello with random nr %d!"), FMath::RandRange(1, 1000)));
}

void AChatClientActor::LogAndDisplayMessage(const FString& Message, const FColor& Color)
{
	// Write to the log for debugging
	UE_LOG(LogTemp, Log, TEXT("%s"), *Message);

	// Display on the screen for quick feedback
	if (GEngine)
	{
		GEngine->AddOnScreenDebugMessage(-1, 5.f, Color, Message);
	}
}