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
        TArray<FWithBsatn<T>>& InsertsRef,
        TArray<FWithBsatn<T>>& DeletesRef,
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

        // Forward ownership of the worker-preprocessed row arrays to avoid rebuilding them on the game thread.
        using FCompactPrimaryKeyTraits = UE::SpacetimeDB::TCompactPrimaryKeyTraits<T>;
        if constexpr (FCompactPrimaryKeyTraits::bEnabled)
        {
            return ClientCache->ApplyDiffByPrimaryKey(
                InTableName,
                MoveTemp(InsertsRef),
                MoveTemp(DeletesRef),
                FCompactPrimaryKeyTraits::GetUniqueIndexName());
        }
        else
        {
            return ClientCache->ApplyDiff(InTableName, MoveTemp(InsertsRef), MoveTemp(DeletesRef));
        }
    }
};
