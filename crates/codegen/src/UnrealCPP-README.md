# SpacetimeDB UnrealCPP Code Generator

This document provides information about the UnrealCPP code generator (`unrealcpp.rs`) and its Blueprint compatibility handling.

## Overview

The UnrealCPP code generator creates Unreal Engine-compatible C++ bindings for SpacetimeDB modules, including:

- **Table classes** with indexing and event handling
- **Reducer classes** with argument structures
- **Type definitions** for module data structures
- **Client connection** management classes
- **Event delegates** for real-time updates

## Blueprint Compatibility

Unreal Engine's Blueprint system has limitations on which C++ types can be exposed. The code generator automatically handles this by detecting incompatible types and adjusting the generated code accordingly.

### Blueprint-Unsupported Types

The following types **cannot** be used in Unreal Engine Blueprints:
- `int8`   (signed   8-bit  integer)
- `int16`  (signed   16-bit integer)
- `uint16` (unsigned 16-bit integer)
- `uint32` (unsigned 32-bit integer) 
- `uint64` (unsigned 64-bit integer)

### Blueprint-Compatible Types

These types **can** be used in Blueprints:

- `bool`
- `int8`, `uint8`, `int16`, `int32`, `int64`
- `float`, `double`
- `FString`
- `All SpacetimeDB SDK types`
- Custom structs and enums
- `TArray<T>` (if T is Blueprint-compatible)

## Code Generation Behavior

### Table Find Functions

When a table has a primary key or unique index with an unsupported type like uint32. In generated code you'll see:

**Generated Code:**
```cpp
// NOTE: Not exposed to Blueprint because uint32 types are not Blueprint-compatible
FMessageType Find(uint32 Key)
{
    return IdIndexHelper.FindUniqueIndex(Key);
}
```

**Behavior:**
- Function is generated without `UFUNCTION(BlueprintCallable)`
- Still fully functional in C++
- Comment explains why Blueprint exposure was omitted

### Reducer Functions

When a reducer has parameters with unsupported types like uint32 and uint64:

**Generated Code:**
```cpp
// NOTE: Not exposed to Blueprint because uint32, uint64 types are not Blueprint-compatible
void SendMessage(const FString& Text, const uint32& Priority, const uint64& Timestamp);
```

**Behavior:**
- Function is generated without `UFUNCTION(BlueprintCallable)`
- Fully functional in C++
- Multiple unsupported types are listed in the comment

### Reducer Event Delegates

When a reducer's event delegate has unsupported parameter types:

**Generated Code:**
```cpp
DECLARE_DYNAMIC_MULTICAST_DELEGATE_FourParams(
    FSendMessageHandler,
    const FReducerEventContext&, Context,
    const FString&, Text,
    const uint32&, Priority,
    const uint64&, Timestamp
);
// NOTE: Not exposed to Blueprint because uint32, uint64 types are not Blueprint-compatible
FSendMessageHandler OnSendMessage;
```

**Behavior:**
- Delegate is generated without `UPROPERTY(BlueprintAssignable)`
- Still bindable from C++
- Blueprint cannot access the event

### Struct Fields

When struct fields have unsupported types:

**Generated Code:**
```cpp
USTRUCT(BlueprintType)
struct MYMODULE_API FSendMessageArgs
{
    GENERATED_BODY()

    UPROPERTY(BlueprintReadWrite, Category="SpacetimeDB")
    FString Text;

    // NOTE: uint32 types can't be used in blueprints
    uint32 Priority;
    
    // NOTE: uint64 types can't be used in blueprints
    uint64 Timestamp;
};
```

**Behavior:**
- Blueprint-compatible fields get `UPROPERTY(BlueprintReadWrite)`
- Unsupported fields are plain C++ members with explanatory comments
- Struct is still `USTRUCT(BlueprintType)` for the supported fields

### Optional Types

SpacetimeDB optional types (`Option<T>`) are generated as custom Unreal structs with special handling:

**Generated Structure:**
```cpp
USTRUCT(BlueprintType)
struct MYMODULE_API FMyModuleOptionalString
{
    GENERATED_BODY()

    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB", meta = (EditCondition = "bHasValue"))
    bool bHasValue = false;

    // Only gets UPROPERTY if the inner type is Blueprint-compatible
    UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "SpacetimeDB", meta = (EditCondition = "bHasValue"))
    FString Value;
    
    // Constructors and helper methods...
};
```

**Behavior:**
- Generated as separate structs in `Public/ModuleBindings/Optionals/` directory
- The `bHasValue` field indicates whether a value is present
- The `Value` field is only editable when `bHasValue` is true (using `EditCondition`)
- If the inner type is not Blueprint-compatible (e.g., `Option<u32>`), the `Value` field won't have `UPROPERTY`
- Custom `GetTypeHash` implementation for proper map/set support
- BSATN serialization support via `UE_SPACETIMEDB_OPTIONAL` macro

### Sum Types (Tagged Enums)

SpacetimeDB sum types (Rust enums with variants) are generated as UStructs + TVarint + BlueprintFunctionLibrary for Blueprint compatibility:

**Generated Structure:**
```cpp
// Tag enum for variant identification
UENUM(BlueprintType)
enum class ECompressableQueryUpdateTag : uint8
{
    Uncompressed,
    Brotli,
    Gzip
};

// Main struct
USTRUCT(BlueprintType)
struct SPACETIMEDBSDK_API FCompressableQueryUpdateType
{
    GENERATED_BODY()

public:
    FCompressableQueryUpdateType() = default;

    TVariant<FQueryUpdateType, TArray<uint8>> MessageData;

    UPROPERTY(BlueprintReadOnly)
    ECompressableQueryUpdateTag Tag;

    static FCompressableQueryUpdateType Uncompressed(const FQueryUpdateType& Value)
    {
        FCompressableQueryUpdateType Obj;
        Obj.Tag = ECompressableQueryUpdateTag::Uncompressed;
        Obj.MessageData.Set<FQueryUpdateType>(Value);
        return Obj;
    }

    static FCompressableQueryUpdateType Brotli(const TArray<uint8>& Value)
    {
        FCompressableQueryUpdateType Obj;
        Obj.Tag = ECompressableQueryUpdateTag::Brotli;
        Obj.MessageData.Set<TArray<uint8>>(Value);
        return Obj;
    }

    static FCompressableQueryUpdateType Gzip(const TArray<uint8>& Value)
    {
        FCompressableQueryUpdateType Obj;
        Obj.Tag = ECompressableQueryUpdateTag::Gzip;
        Obj.MessageData.Set<TArray<uint8>>(Value);
        return Obj;
    }

    // Is* functions
    bool IsUncompressed() const { return Tag == ECompressableQueryUpdateTag::Uncompressed; }

    // GetAs* functions
    FQueryUpdateType GetAsUncompressed() const
    {
        ensureMsgf(IsUncompressed(), TEXT("MessageData does not hold Uncompressed!"));
        return MessageData.Get<FQueryUpdateType>();
    }
};

// Corresponding blueprint function library for using the sum types
UCLASS()
class SPACETIMEDBSDK_API UCompressableQueryUpdateBpLib : public UBlueprintFunctionLibrary
{
    GENERATED_BODY()

private:
    UFUNCTION(BlueprintCallable, Category = "SpacetimeDB|CompressableQueryUpdate")
    static FCompressableQueryUpdateType Uncompressed(const FQueryUpdateType& InValue)
    {
        return FCompressableQueryUpdateType::Uncompressed(InValue);
    }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|CompressableQueryUpdate")
    static bool IsUncompressed(const FCompressableQueryUpdateType& InValue) { return InValue.IsUncompressed(); }

    UFUNCTION(BlueprintPure, Category = "SpacetimeDB|CompressableQueryUpdate")
    static FQueryUpdateType GetAsUncompressed(const FCompressableQueryUpdateType& InValue)
    {
        return InValue.GetAsUncompressed();
    }

    // Rest is the same for other variants...
}
```

**Key Behaviors:**

- **UStruct + TVarint - based**: Generated as UStruct + TVarint for saving memory
- **TVarint**: Uses Unreal's TVarint to store variant payload
- **Blueprint Function Library**: contains a bunch of private static BlueprintCallable functions. To instantiate and use varints in BPs.
- **Type Safety**: Each variant gets its own factory function and getter
- **Memory Overhead**: C++ unions based
- **Unit Variants**: Variants without payload use `FSpacetimeDBUnit` type
- **Blueprint Compatible**: All functions are BlueprintCallable or BlueprintPure for full Blueprint access
- **BSATN Support**: Generated with proper serialization macros

### Plain Enums (Simple Enums)

SpacetimeDB plain enums (Rust enums with only unit variants, no payloads) are generated as simple Unreal Engine enums:

**Rust Definition:**
```rust
#[derive(...)]
pub enum Status {
    Pending,
    Active,
    Inactive,
    Suspended,
}
```

**Generated Code:**
```cpp
UENUM(BlueprintType)
enum class EStatusType : uint8
{
    Pending,
    Active,
    Inactive,
    Suspended,
};
```

**Key Behaviors:**

- **Simple UENUM**: Generated as standard Unreal Engine enum class with `uint8` backing type
- **Blueprint Compatible**: `UENUM(BlueprintType)` makes it fully accessible in Blueprints
- **Naming Convention**: `E[Name]Type` format (e.g., `EStatusType`)
- **PascalCase Variants**: All variants converted to PascalCase for Unreal conventions
- **Lightweight**: No memory overhead - just a simple enum value
- **Type Safety**: Strongly typed enum class prevents implicit conversions

**Usage in Generated Code:**
- Used directly as `EStatusType` in struct fields, function parameters, etc.
- Can be used in Blueprint dropdown selections, switch statements, etc.
- Fully compatible with Unreal's reflection and serialization systems

**Comparison with Sum Types:**
- **Plain Enums**: Simple `UENUM` with no payload → lightweight, direct Blueprint usage
- **Sum Types**: Complex `UStructs` with `TVarint` payload → union based, indirect Blueprint usage through BPLib

## Generating Module Bindings

To generate UnrealCPP bindings for your SpacetimeDB module, use the SpacetimeDB CLI:

### Basic Command

```bash
cargo run --bin spacetimedb-cli -- generate --lang unrealcpp --uproject-dir <uproject_directory> --project-path <module_path> --module-name <ModuleName>
```

### Example

```bash
cargo run --bin spacetimedb-cli -- generate --lang unrealcpp --uproject-dir crates/sdk-unreal/examples/QuickstartChat --project-path modules/quickstart-chat --module-name QuickstartChat
```

### Parameters

- `--lang unrealcpp`: Specifies the UnrealCPP code generator
- `--uproject-dir`: Directory containing Unreal's .uproject or .uplugin file
- `--project-path`: Path to your SpacetimeDB module source code
- `--module-name`: **Required** - Name used for generated classes, API prefix and putting generated module bindings in the correct Module's Source

### Why Module Name is Required

The `--module-name` parameter is **mandatory** for UnrealCPP generation because:

1. **Unreal Engine API Macro**: Generated classes use `MODULENAME_API` macros (e.g., `QUICKSTARTCHAT_API`) for proper DLL export/import in Unreal Engine
2. **Class Prefixing**: All the optional generated classes are prefixed with the module name to avoid naming conflicts (e.g., `FQuickstartChatOptionalString`)
3. **Build System Integration**: Unreal Engine's build system requires proper API macros for linking across modules
4. **Generated Module Bindings**: Put generated bindings in correct module's source

**⚠️ IMPORTANT:** Without the module name, the generated code would not compile in Unreal Engine due to missing API macros and naming conflicts.

## Implementation Details

The Blueprint compatibility checking is implemented in the `is_blueprintable()` function, which recursively checks:

The code generator collects incompatible types during the first parameter iteration to avoid duplicate loops and provides specific error messages listing exactly which types are causing Blueprint incompatibility.

## Error Messages

All error messages follow a consistent format:

- **Single type:** `"uint32 types are not Blueprint-compatible"`
- **Multiple types:** `"uint32, uint64 types are not Blueprint-compatible"`

This makes it clear to developers exactly which types need to be changed for Blueprint compatibility.