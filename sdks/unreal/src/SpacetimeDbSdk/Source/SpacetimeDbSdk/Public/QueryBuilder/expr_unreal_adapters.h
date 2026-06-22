#pragma once

#include "CoreMinimal.h"
#include "Types/Builtins.h"

inline std::string literal_sql(const TCHAR* value) {
    return quote_string(value == nullptr ? "" : TCHAR_TO_UTF8(value));
}

inline std::string literal_sql(const FString& value) {
    return quote_string(TCHAR_TO_UTF8(*value));
}

inline std::string ensure_hex_prefix(std::string value) {
    if (value.rfind("0x", 0) == 0 || value.rfind("0X", 0) == 0) {
        return value;
    }
    return "0x" + value;
}

inline std::string literal_sql(const FSpacetimeDBIdentity& value) {
    return ensure_hex_prefix(std::string(TCHAR_TO_UTF8(*value.ToHex())));
}

inline std::string literal_sql(const FSpacetimeDBConnectionId& value) {
    return ensure_hex_prefix(std::string(TCHAR_TO_UTF8(*value.ToHex())));
}

inline std::string literal_sql(const FSpacetimeDBUuid& value) {
    return quote_string(TCHAR_TO_UTF8(*value.ToString()));
}

inline std::string literal_sql(const FSpacetimeDBTimestamp& value) {
    return quote_string(trim_timestamp_fraction(TCHAR_TO_UTF8(*value.ToString())));
}

std::string literal_sql(const FSpacetimeDBTimeDuration& value) = delete;

inline std::string literal_sql(const TArray<uint8>& value) {
    std::ostringstream out;
    out << "0x" << std::hex << std::setfill('0');
    for (uint8 byte : value) {
        out << std::setw(2) << static_cast<unsigned>(byte);
    }
    return out.str();
}

inline std::string literal_sql(const FSpacetimeDBUInt128& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBInt128& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBUInt256& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
inline std::string literal_sql(const FSpacetimeDBInt256& value) { return TCHAR_TO_UTF8(*value.ToDecimalString()); }
