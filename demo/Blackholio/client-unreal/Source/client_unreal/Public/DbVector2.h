#pragma once

#include "ModuleBindings/Types/DbVector2Type.g.h"

FORCEINLINE FDbVector2Type ToDbVector(const FVector2D& Vec)
{
	FDbVector2Type Out;
	Out.X = Vec.X;
	Out.Y = Vec.Y;
	return Out;
}

FORCEINLINE FDbVector2Type ToDbVector(const FVector& Vec)
{
	FDbVector2Type Out;
	Out.X = Vec.X;
	Out.Y = Vec.Y;
	return Out;
}

FORCEINLINE FVector2D ToFVector2D(const FDbVector2Type& Vec)
{
	return FVector2D(Vec.X * 100.f, Vec.Y * 100.f);
}

FORCEINLINE FVector ToFVector(const FDbVector2Type& Vec, float Z = 0.f)
{
	return FVector(Vec.X * 100.f, Z, Vec.Y * 100.f);
}
