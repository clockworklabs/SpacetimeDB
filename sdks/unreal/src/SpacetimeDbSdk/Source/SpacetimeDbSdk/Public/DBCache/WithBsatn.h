#pragma once
#include "CoreMinimal.h"

/** Wrapper holding a row and its BSATN serialized bytes */
template<typename RowType>
struct FWithBsatn
{
    /** Serialized BSATN bytes for this row */
    TArray<uint8> Bsatn;
    /** Deserialized row value */
    RowType Row;

    FWithBsatn() = default;
    FWithBsatn(const TArray<uint8>& InBsatn, const RowType& InRow)
        : Bsatn(InBsatn), Row(InRow) {
    }
};