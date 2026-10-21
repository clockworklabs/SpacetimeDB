#pragma once

/**
 * Actor used to drive the Quickstart chat sample.
 * Handles connecting to SpacetimeDB and exposing
 * a set of convenience Blueprint functions for the
 * test level.
 */

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Connection/Credentials.h"
#include "ModuleBindings/SpacetimeDBClient.g.h"

#include "ModuleBindings/Tables/MessageTable.g.h"
#include "ModuleBindings/Tables/UserTable.g.h"

#include "ChatClientActor.generated.h"


 /**
  * Simple client actor used by the sample project. It owns the
  * database connection and exposes helper Blueprint callable
  * functions to exercise the chat reducers and subscriptions.
  */
UCLASS(BlueprintType)
class QUICKSTARTCHAT_API AChatClientActor : public AActor
{
    GENERATED_BODY()

public:
    AChatClientActor();

    // -------------------------------------------------------------------------
    // Utility
    // -------------------------------------------------------------------------

    /** If true the actor will attempt to maintain the database connection */
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB")
    bool bActive = true;


    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void PrintCurrentUser();

    // -------------------------------------------------------------------------
    // Reducer callers
    // -------------------------------------------------------------------------

    /** Calls the SetName reducer with the specified value */
    UFUNCTION()
    void ReducerSetName(const FString NewName);

    /** Picks a random name and calls the SetName reducer */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void ReducerSetRandomName();

    // -------------------------------------------------------------------------
    // Reducer helpers (Can be called in editor)
    // -------------------------------------------------------------------------

    /** Sends a chat message via reducer */
    UFUNCTION()
    void ReducerSendMessage(const FString Text);

    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void ReducerSendRandomMessage();

    // -------------------------------------------------------------------------
    // Subsctiption Control (Can be called in editor)
    // -------------------------------------------------------------------------

    /** Subscribe to all tables in the demo schema */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void SubscribeToAll();

    /** Subscribe to user updates only */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void SubscribeToUser();

    /** Subscribe to message table updates */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void SubscribeToMessage();

    /** Unsubscribe from all active subscriptions */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void UnsubscribeFromAll();

    /** Unsubscribe from user table */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void UnsubscribeFromUser();

    /** Unsubscribe from message table */
    UFUNCTION(CallInEditor, Category = "SpacetimeDB")
    void UnsubscribeFromMessage();


protected:

    // -------------------------------------------------------------------------
    // Actor Lifecycle
    // -------------------------------------------------------------------------

    /** Called when the actor enters the world */
    virtual void BeginPlay() override;
    /** Clean up the database connection on shutdown */
    virtual void EndPlay(const EEndPlayReason::Type EndPlayReason) override;
    /** Ticks the active connection */
    virtual void Tick(float DeltaSeconds) override;

private:

    // -------------------------------------------------------------------------
    // Connection object
    // -------------------------------------------------------------------------

    /** Live database connection instance */
    UPROPERTY()
    UDbConnection* Conn;

    // -------------------------------------------------------------------------
    // Subscription handlers
    // -------------------------------------------------------------------------

    /** Handle to the "all" subscription */
    UPROPERTY()
    USubscriptionHandle* SubscriptionHandleAll;

    /** Handle to the user table subscription */
    UPROPERTY()
    USubscriptionHandle* SubscriptionHandleUser;

    /** Handle to the message table subscription */
    UPROPERTY()
    USubscriptionHandle* SubscriptionHandleMessage;

    // -------------------------------------------------------------------------
    // Variables
    // -------------------------------------------------------------------------

    /** Local client identity returned from the server on connect */
    FSpacetimeDBIdentity LocalIdentity;

    /** Configured host name */
    FString Host;

    /** Database name */
    FString DbName;

    // -------------------------------------------------------------------------
    // Registration
    // -------------------------------------------------------------------------

    /** Registers all SpacetimeDB callbacks */
    UFUNCTION()
    void RegisterCallbacks();

    // -------------------------------------------------------------------------
    // Connection callbacks
    // -------------------------------------------------------------------------

    /** Callback for successful connection */
    UFUNCTION()
    void OnConnected(UDbConnection* Connection, FSpacetimeDBIdentity Identity, const FString& Token);

    /** Called when the connection is closed */
    UFUNCTION()
    void OnDisconnected(UDbConnection* Connection, const FString& ErrorMessage);

    /** Called when the initial connection fails */
    UFUNCTION()
    void OnConnectError(const FString& ErrorMessage);


    // -------------------------------------------------------------------------
   // Subsctription updates
   // -------------------------------------------------------------------------

    /** Fired when a subscription is successfully applied */
    UFUNCTION()
    void OnSubscriptionApplied(FSubscriptionEventContext& Ctx);

    /** Fired when applying a subscription fails */
    UFUNCTION()
    void OnSubscriptionError(const FErrorContext& Ctx);


    // -------------------------------------------------------------------------
    // Table changes updates
    // -------------------------------------------------------------------------

    /** Called when a user row is inserted */
    UFUNCTION()
    void OnUserInsert(const FEventContext& Context, const FUserType& NewRow);
    /** Called when a user row is updated */
    UFUNCTION()
    void OnUserUpdate(const FEventContext& Context, const FUserType& OldRow, const FUserType& NewRow);
    /** Called when a user row is deleted */
    UFUNCTION()
    void OnUserDelete(const FEventContext& Context, const FUserType& RemovedRow);

    /** Called when a message row is inserted */
    UFUNCTION()
    void OnMessageInsert(const FEventContext& Context, const FMessageType& NewRow);
    /** Called when a message row is updated */
    UFUNCTION()
    void OnMessageUpdate(const FEventContext& Context, const FMessageType& OldRow, const FMessageType& NewRow);
    /** Called when a message row is deleted */
    UFUNCTION()
    void OnMessageDelete(const FEventContext& Context, const FMessageType& DeletedRow);

   // -------------------------------------------------------------------------
   // Reducer updates
   // -------------------------------------------------------------------------

    /** Catch-all reducer error handler */
    UFUNCTION()
    void OnUnhandledReducerError(const FReducerEventContext& Context, const FString& Error);

    /** Handler for SetName reducer result */
    UFUNCTION()
    void OnReducerOnSetName(const FReducerEventContext& Context, const FString& Name);

    /** Handler for SendMessage reducer result */
    UFUNCTION()
    void OnReducerOnSendMessage(const FReducerEventContext& Context, const FString& Text);

    // -------------------------------------------------------------------------
   // Helper functions
   // -------------------------------------------------------------------------

    /** Print all messages in message table in incoming time order */
    UFUNCTION()
    void PrintMessagesInOrder();

    /** Print a message and who sent it */
    UFUNCTION()
    void PrintMessage(const FMessageType& Message);

    /** Get user name if set else get part of identification */
    UFUNCTION()
    FString UserNameOrIdentity(const FUserType& User) const;

    /** Logs a string to the output log and screen */
    UFUNCTION(BlueprintCallable, Category = "Debug")
    void LogAndDisplayMessage(const FString& Message, const FColor& Color);

};