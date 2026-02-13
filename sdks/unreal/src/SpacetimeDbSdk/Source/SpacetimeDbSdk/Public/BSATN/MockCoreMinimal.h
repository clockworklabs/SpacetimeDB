/**
 * @file MockCoreMinimal.h
 * @brief Mock Unreal Engine types for testing/examples
 * 
 * This file provides minimal mock implementations of Unreal Engine types
 * for testing the BSATN wrapper without requiring the full Unreal Engine.
 * In a real UE project, use the actual CoreMinimal.h instead.
 */

#pragma once

#include <string>
#include <vector>
#include <optional>
#include <iostream>
#include <sstream>
#include <cmath>
#include <cstring>
#include <cstdint>
#include <cstdio>

// Mock implementation of core UE types
#define TEXT(x) x
#define TCHAR char
#define FTCHARToUTF8 MockFTCHARToUTF8
#define UTF8_TO_TCHAR(x) x

// Mock UTF8 converter
struct MockFTCHARToUTF8 {
    const char* str;
    MockFTCHARToUTF8(const char* s) : str(s) {}
    const char* Get() const { return str; }
    size_t Length() const { return strlen(str); }
};

// TArray - Dynamic array similar to std::vector
template<typename T, typename Allocator = std::allocator<T>>
class TArray : public std::vector<T, Allocator> {
public:
    using std::vector<T, Allocator>::vector;
    
    int32_t Num() const { return static_cast<int32_t>(this->size()); }
    void Add(const T& item) { this->push_back(item); }
    void Add(T&& item) { this->push_back(std::move(item)); }
    void Reserve(int32_t count) { this->reserve(count); }
    T* GetData() { return this->data(); }
    const T* GetData() const { return this->data(); }
};

// TOptional - Optional value similar to std::optional
template<typename T>
class TOptional : public std::optional<T> {
public:
    using std::optional<T>::optional;
    
    bool IsSet() const { return this->has_value(); }
    const T& GetValue() const { return this->value(); }
    T& GetValue() { return this->value(); }
};

// FString - String class
class FString : public std::string {
public:
    using std::string::string;
    FString() = default;
    FString(const char* str) : std::string(str) {}
    FString(const std::string& str) : std::string(str) {}
    
    const char* operator*() const { return c_str(); }
    bool operator==(const FString& other) const { 
        return static_cast<const std::string&>(*this) == static_cast<const std::string&>(other);
    }
    bool operator==(const char* other) const {
        return static_cast<const std::string&>(*this) == other;
    }
    
    // Static Printf method
    template<typename... Args>
    static FString Printf(const char* format, Args... args) {
        char buffer[512];
        std::snprintf(buffer, sizeof(buffer), format, args...);
        return FString(buffer);
    }
    
    // String concatenation
    FString& operator+=(const char* str) {
        static_cast<std::string&>(*this) += str;
        return *this;
    }
};

// FName - Immutable name/identifier
class FName {
    FString name;
public:
    FName() = default;
    FName(const FString& str) : name(str) {}
    FName(const char* str) : name(str) {}
    
    FString ToString() const { return name; }
    bool operator==(const FName& other) const { return name == other.name; }
};

// FVector - 3D vector
struct FVector {
    float X, Y, Z;
    
    FVector() : X(0), Y(0), Z(0) {}
    FVector(float x, float y, float z) : X(x), Y(y), Z(z) {}
    
    bool operator==(const FVector& other) const {
        return std::abs(X - other.X) < 0.0001f &&
               std::abs(Y - other.Y) < 0.0001f &&
               std::abs(Z - other.Z) < 0.0001f;
    }
};

// FRotator - Rotation
struct FRotator {
    float Pitch, Yaw, Roll;
    
    FRotator() : Pitch(0), Yaw(0), Roll(0) {}
    FRotator(float pitch, float yaw, float roll) : Pitch(pitch), Yaw(yaw), Roll(roll) {}
    
    bool operator==(const FRotator& other) const {
        return std::abs(Pitch - other.Pitch) < 0.0001f &&
               std::abs(Yaw - other.Yaw) < 0.0001f &&
               std::abs(Roll - other.Roll) < 0.0001f;
    }
};

// FTransform - Transform (position, rotation, scale)
struct FTransform {
    FVector Translation;
    FRotator Rotation;
    FVector Scale3D;
    
    FTransform() : Scale3D(1, 1, 1) {}
    FTransform(const FVector& trans, const FRotator& rot, const FVector& scale)
        : Translation(trans), Rotation(rot), Scale3D(scale) {}
};

// FGuid - Globally unique identifier
struct FGuid {
    uint32_t A, B, C, D;
    
    FGuid() : A(0), B(0), C(0), D(0) {}
    FGuid(uint32_t a, uint32_t b, uint32_t c, uint32_t d) : A(a), B(b), C(c), D(d) {}
    
    bool operator==(const FGuid& other) const {
        return A == other.A && B == other.B && C == other.C && D == other.D;
    }
    
    // Simple mock NewGuid - not truly unique, just for testing
    static FGuid NewGuid() {
        static uint32_t counter = 1;
        return FGuid(counter++, counter++, counter++, counter++);
    }
    
    FString ToString() const {
        char buffer[64];
        std::snprintf(buffer, sizeof(buffer), "%08X-%08X-%08X-%08X", A, B, C, D);
        return FString(buffer);
    }
};

// FDateTime - Date and time
class FDateTime {
    int64_t ticks;
public:
    FDateTime() : ticks(0) {}
    explicit FDateTime(int64_t t) : ticks(t) {}
    
    int64_t GetTicks() const { return ticks; }
    
    // Simple mock Now - just returns incrementing value
    static FDateTime Now() {
        static int64_t counter = 637890123456789;
        return FDateTime(counter++);
    }
};

// FTimespan - Time duration
class FTimespan {
    int64_t ticks;
public:
    FTimespan() : ticks(0) {}
    explicit FTimespan(int64_t t) : ticks(t) {}
    
    int64_t GetTicks() const { return ticks; }
};

// Common type aliases
using uint8 = uint8_t;
using int32 = int32_t;
using uint32 = uint32_t;
using int64 = int64_t;
using uint64 = uint64_t;

// SpacetimeDB type aliases for UE compatibility
namespace SpacetimeDb {
    class u128;
    class i128;
    class u256;
    class i256;
    class Identity;
    class ConnectionId;
    class Timestamp;
    class TimeDuration;
}