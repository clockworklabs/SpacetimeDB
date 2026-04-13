#pragma once

#include "CoreMinimal.h"
#include "ProcedureFlags.generated.h"

/** Flags controlling procedure call behavior */
UENUM(BlueprintType)
enum class EProcedureFlags : uint8
{
    /** Default behavior - server will send full update and success notification */
    Default UMETA(DisplayName = "Default"),

};