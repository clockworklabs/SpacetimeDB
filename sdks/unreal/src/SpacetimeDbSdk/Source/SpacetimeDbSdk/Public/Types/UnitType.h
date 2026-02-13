#pragma once
#include "CoreMinimal.h"
#include "BSATN/UESpacetimeDB.h"
#include "UnitType.generated.h"

/**
 * Represents a Unit type, which holds no data.
 * It is used as a placeholder in data structures (like tagged unions) for variants
 * that have no associated value.
 */
USTRUCT(BlueprintType, Category = "SpacetimeDB")
struct FSpacetimeDBUnit
{
    GENERATED_BODY()

    /**
     * Compare two Unit types for equality. Always returns true.
     * @param Other The other Unit to compare against.
     * @return True, as all instances of Unit are identical.
     */
    bool operator==(const FSpacetimeDBUnit& Other) const
    {
        return true;
    }

    /**
     * Compare two Unit types for inequality. Always returns false.
     * @param Other The other Unit to compare against.
     * @return False, as all instances of Unit are identical.
     */
    bool operator!=(const FSpacetimeDBUnit& Other) const
    {
        return false;
    }

};

FORCEINLINE uint32 GetTypeHash(const FSpacetimeDBUnit& /*Value*/)
{
    return 0u;
}


namespace UE::SpacetimeDB
{
    UE_SPACETIMEDB_STRUCT_EMPTY(FSpacetimeDBUnit);
}