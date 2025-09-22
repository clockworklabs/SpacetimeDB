#include "Connection/Credentials.h"
#include "Misc/Paths.h"
#include "Misc/ConfigCacheIni.h"

FString UCredentials::Token;
FString UCredentials::StoredKey;

void UCredentials::Init(const FString& InFilename)
{
    StoredKey = InFilename;
    LoadToken();
}

FString UCredentials::LoadToken()
{
    FString LoadedValue;
    if (StoredKey.IsEmpty())
    {
        UE_LOG(LogTemp, Warning, TEXT("UCredentials::Init has not been called before LoadToken."));
        return Token;
    }

    if (GConfig->GetString(TEXT("SpacetimeDB"), *StoredKey, LoadedValue, GGameUserSettingsIni))
    {
        Token = LoadedValue;
        UE_LOG(LogTemp, Verbose, TEXT("UCredentials::Credentials loaded for key %s from %s."), *StoredKey, *FPaths::GetCleanFilename(GGameUserSettingsIni));
    }
    else
    {
        UE_LOG(LogTemp, Verbose, TEXT("UCredentials::No stored credentials found for key %s."), *StoredKey);
    }

    return Token;
}

void UCredentials::SaveToken(const FString& InToken)
{
    Token = InToken;

    if (StoredKey.IsEmpty())
    {
        UE_LOG(LogTemp, Warning, TEXT("UCredentials::Init has not been called before SaveToken."));
        return;
    }

    GConfig->SetString(TEXT("SpacetimeDB"), *StoredKey, *Token, GGameUserSettingsIni);

    // This call writes the in-memory changes to the GGameUserSettingsIni file on the disk.
    GConfig->Flush(false, GGameUserSettingsIni);
}
