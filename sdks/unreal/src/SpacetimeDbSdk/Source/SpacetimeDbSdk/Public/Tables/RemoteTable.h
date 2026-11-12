#pragma once

#include "CoreMinimal.h"
#include "UObject/Object.h"
#include "DBCache/ClientCache.h"
#include "DBCache/TableAppliedDiff.h"
#include "DBCache/WithBsatn.h"

#include "RemoteTable.generated.h"

/**
 * Base type for all generated remote table wrappers.
 * Provides helper functionality for applying diffs
 * received from the server.
 */
UCLASS()
class SPACETIMEDBSDK_API URemoteTable : public UObject
{
    GENERATED_BODY()

protected:

    /**
     * Apply a diff to the local cache.
     * @param InsertsRef Insert operations with BSATN encoded keys
     * @param DeletesRef Delete operations with BSATN encoded keys
     * @param ClientCache Cache instance for this table
     * @param InTableName Name of the table being updated
     */
    template<typename T>
    FTableAppliedDiff<T> BaseUpdate(
        const TArray<FWithBsatn<T>>& InsertsRef,
        const TArray<FWithBsatn<T>>& DeletesRef,
        const TSharedPtr<UClientCache<T>>& ClientCache,
        const FString& InTableName
    )
    {
		// Validate the client cache before proceeding
        if (!ClientCache.IsValid())
        {
            UE_LOG(LogTemp, Error, TEXT("RemoteTable::BaseUpdate called with invalid ClientCache for table %s"), *InTableName);
            return {};
        }
        // Generated tables change their cache only through here, so a table that never got here has nothing cached.
        ObservedCache = ClientCache;

        TArray<TPair<TArray<uint8>, T>> Inserts;
        for (const FWithBsatn<T>& Insert : InsertsRef)
        {
            Inserts.Add({ Insert.Bsatn, Insert.Row });
        }

        TArray<TArray<uint8>> Deletes;
        for (const FWithBsatn<T>& Delete : DeletesRef)
        {
            Deletes.Add(Delete.Bsatn);
        }

        // Forward to the shared client cache implementation
        return ClientCache->ApplyDiff(InTableName, Inserts, Deletes);
    }

private:
    // Reconnect replay reads cached rows here instead of through the generated Data member, so existing
    // generated bindings keep compiling.
    friend class UDbConnectionBase;

    // Only sees caches written through BaseUpdate. Any new write path must go through it too, or replay will miss
    // those rows.
    template<typename T>
    TSharedPtr<UClientCache<T>> GetObservedCache() const
    {
        return StaticCastSharedPtr<UClientCache<T>>(ObservedCache.Pin());
    }

    TWeakPtr<FClientCacheBase> ObservedCache;
};