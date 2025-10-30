/**
 * @file UESpacetimeDB.h
 * @brief BSATN Serialization Wrapper for Unreal Engine Types
 *
 * This header provides a compatibility layer between SpacetimeDB's BSATN
 * (Binary SpacetimeDB Abstract Type Notation) serialization system and
 * Unreal Engine's type system. It enables seamless serialization and
 * deserialization of UE data structures for use with SpacetimeDB.
 *
 * Key Features:
 * - Zero-copy serialization where possible
 * - Support for all common UE types (FString, TArray, FVector, etc.)
 * - Simple macro-based API for custom structs
 * - Type-safe compile-time serialization
 *
 * Usage Example:
 * @code
 *   // Serialize a UE type
 *   FVector position(100, 200, 300);
 *   TArray<uint8> serialized = UE::SpacetimeDB::Serialize(position);
 *
 *   // Deserialize back
 *   FVector deserialized = UE::SpacetimeDB::Deserialize<FVector>(serialized);
 * @endcode
 */

#pragma once

#include "Core/bsatn.h"

#include "CoreMinimal.h"
#include "Math/Quat.h"
#include "Types/LargeIntegers.h"

#include <chrono>
#include <array>
#include <typeinfo>

namespace UE::SpacetimeDB {

	// =============================================================================
	// Forward Declarations
	// =============================================================================

	class UEWriter;
	class UEReader;
	template<typename T> void serialize(UEWriter& w, const T& value);
	template<typename T> T deserialize(UEReader& r);

	// =============================================================================
	// UEWriter - BSATN Writer Wrapper for Unreal Engine
	// =============================================================================

	/**
	 * @class UEWriter
	 * @brief Wrapper around SpacetimeDB's BSATN Writer for Unreal Engine types
	 *
	 * This class provides a UE-friendly interface to the BSATN serialization system,
	 * handling conversions between UE types (FString, TArray) and standard C++ types.
	 *
	 * The writer maintains an internal buffer that accumulates serialized data.
	 * Use take_buffer() to extract the final result.
	 */
	class UEWriter {
	private:
		::SpacetimeDb::bsatn::Writer core_writer;  ///< Underlying BSATN writer

	public:
		UEWriter() = default;

		// -------------------------------------------------------------------------
		// Primitive Type Writers
		// -------------------------------------------------------------------------

		/** @name Boolean and Integer Writers
		 *  Write primitive integer types to the buffer
		 */
		 ///@{
		void write_bool(bool value) { core_writer.write_bool(value); }
		void write_u8(uint8_t value) { core_writer.write_u8(value); }
		void write_u16(uint16_t value) { core_writer.write_u16_le(value); }
		void write_u32(uint32_t value) { core_writer.write_u32_le(value); }
		void write_u64(uint64_t value) { core_writer.write_u64_le(value); }
		void write_i8(int8_t value) { core_writer.write_i8(value); }
		void write_i16(int16_t value) { core_writer.write_i16_le(value); }
		void write_i32(int32_t value) { core_writer.write_i32_le(value); }
		void write_i64(int64_t value) { core_writer.write_i64_le(value); }
		///@}

		/** @name Floating Point Writers
		 *  Write floating point values to the buffer
		 */
		 ///@{
		void write_f32(float value) { core_writer.write_f32_le(value); }
		void write_f64(double value) { core_writer.write_f64_le(value); }
		///@}

		// -------------------------------------------------------------------------
		// UE-Specific Type Writers
		// -------------------------------------------------------------------------

		/**
		 * Write a UE FString as UTF-8 encoded string
		 * @param str The FString to serialize
		 */
		void write_string(const FString& str) {
			FTCHARToUTF8 converter(*str);
			core_writer.write_string(std::string(converter.Get(), converter.Length()));
		}

		/**
		 * Write a TArray<uint8> as raw bytes with length prefix
		 * @param arr The byte array to serialize
		 */
		void write_array_u8(const TArray<uint8>& arr) {
			core_writer.write_u32_le(static_cast<uint32_t>(arr.Num()));
			if (arr.Num() > 0) {
				core_writer.write_bytes(std::vector<uint8_t>(arr.GetData(), arr.GetData() + arr.Num()));
			}
		}

		/**
		 * Write a generic TArray with length prefix
		 * @tparam T The element type
		 * @param arr The array to serialize
		 * @note Implementation is defined after serialize() declarations
		 */
		template<typename T>
		void write_array(const TArray<T>& arr);

		// -------------------------------------------------------------------------
		// Buffer Management
		// -------------------------------------------------------------------------

		/**
		 * Extract the serialized buffer as a UE TArray
		 * @return TArray<uint8> containing the serialized data
		 * @note This consumes the writer (move semantics)
		 */
		TArray<uint8> take_buffer()&& {
			auto std_buffer = std::move(core_writer).take_buffer();
			TArray<uint8> ue_buffer;
			ue_buffer.Reserve(static_cast<int32_t>(std_buffer.size()));
			for (auto byte : std_buffer) {
				ue_buffer.Add(byte);
			}
			return ue_buffer;
		}

		/**
		 * Get a reference to the internal buffer (for compatibility)
		 * @return Reference to the internal std::vector buffer
		 */
		const std::vector<uint8_t>& get_std_buffer() const {
			return core_writer.get_buffer();
		}
	};

	// =============================================================================
	// UEReader - BSATN Reader Wrapper for Unreal Engine
	// =============================================================================

	/**
	 * @class UEReader
	 * @brief Wrapper around SpacetimeDB's BSATN Reader for Unreal Engine types
	 *
	 * This class provides a UE-friendly interface for deserializing BSATN data,
	 * handling conversions between standard C++ types and UE types.
	 *
	 * The reader stores a copy of the input data to prevent lifetime issues
	 * that could occur if the original TArray is destroyed.
	 */
	class UEReader {
	private:
		std::vector<uint8_t> stored_data;           ///< Local copy of data to ensure lifetime
		::SpacetimeDb::bsatn::Reader core_reader;   ///< Underlying BSATN reader

	public:
		/**
		 * Construct a reader from a UE byte array
		 * @param data The TArray<uint8> containing serialized BSATN data
		 * @note Makes a copy of the data to prevent lifetime issues
		 */
		explicit UEReader(const TArray<uint8>& data)
			: stored_data(data.GetData(), data.GetData() + data.Num()),
			core_reader(stored_data) {
			// Debug output (can be removed in production)
			if (data.Num() > 0) {
				//std::cout << "  UEReader initialized with data[0] = " << std::hex << (int)data[0] << std::dec << std::endl;
			}
		}

		/**
		 * Construct a reader from a standard vector
		 * @param data The vector containing serialized BSATN data
		 */
		explicit UEReader(const std::vector<uint8_t>& data)
			: stored_data(data),
			core_reader(stored_data) {}

		// -------------------------------------------------------------------------
		// Primitive Type Readers
		// -------------------------------------------------------------------------

		/** @name Boolean and Integer Readers
		 *  Read primitive integer types from the buffer
		 */
		 ///@{
		bool read_bool() {
			return core_reader.read_bool();
		}

		uint8_t read_u8() { return core_reader.read_u8(); }
		uint16_t read_u16() { return core_reader.read_u16_le(); }
		uint32_t read_u32() { return core_reader.read_u32_le(); }
		uint64_t read_u64() { return core_reader.read_u64_le(); }
		int8_t read_i8() { return core_reader.read_i8(); }
		int16_t read_i16() { return core_reader.read_i16_le(); }
		int32_t read_i32() { return core_reader.read_i32_le(); }
		int64_t read_i64() { return core_reader.read_i64_le(); }
		///@}

		/** @name Floating Point Readers
		 *  Read floating point values from the buffer
		 */
		 ///@{
		float read_f32() { return core_reader.read_f32_le(); }
		double read_f64() { return core_reader.read_f64_le(); }
		///@}

		// -------------------------------------------------------------------------
		// UE-Specific Type Readers
		// -------------------------------------------------------------------------

		/**
		 * Read a UTF-8 string and convert to FString
		 * @return FString containing the deserialized string
		 */
		FString read_string() {
			auto std_str = core_reader.read_string();
			return FString(UTF8_TO_TCHAR(std_str.c_str()));
		}

		/**
		 * Read a byte array with length prefix
		 * @return TArray<uint8> containing the deserialized bytes
		 */
		TArray<uint8> read_array_u8() {
			uint32_t count = core_reader.read_u32_le();
			TArray<uint8> result;
			result.Reserve(count);
			for (uint32_t i = 0; i < count; ++i) {
				result.Add(core_reader.read_u8());
			}
			return result;
		}

		/**
		 * Read a generic TArray with length prefix
		 * @tparam T The element type
		 * @return TArray<T> containing the deserialized elements
		 * @note Implementation is defined after deserialize() declarations
		 */
		template<typename T>
		TArray<T> read_array();
	};

	// =============================================================================
	// Primitive Type Serialization
	// =============================================================================

	/**
	 * @defgroup PrimitiveSerialization Primitive Type Serialization
	 * @brief Serialization functions for C++ primitive types
	 * @{
	 */

	inline void serialize(UEWriter& w, bool value) { w.write_bool(value); }
	inline void serialize(UEWriter& w, uint8 value) { w.write_u8(value); }
	inline void serialize(UEWriter& w, uint16 value) { w.write_u16(value); }
	inline void serialize(UEWriter& w, uint32 value) { w.write_u32(value); }
	inline void serialize(UEWriter& w, uint64 value) { w.write_u64(value); }
	inline void serialize(UEWriter& w, int8 value) { w.write_i8(value); }
	inline void serialize(UEWriter& w, int16 value) { w.write_i16(value); }
	inline void serialize(UEWriter& w, int32 value) { w.write_i32(value); }
	inline void serialize(UEWriter& w, int64 value) { w.write_i64(value); }
	inline void serialize(UEWriter& w, float value) { w.write_f32(value); }
	inline void serialize(UEWriter& w, double value) { w.write_f64(value); }

	/** @} */ // end of PrimitiveSerialization group

	// =============================================================================
	// Primitive Type Deserialization
	// =============================================================================

	/**
	 * @defgroup PrimitiveDeserialization Primitive Type Deserialization
	 * @brief Deserialization functions for C++ primitive types
	 * @{
	 */

	template<> inline bool deserialize<bool>(UEReader& r) { return r.read_bool(); }
	template<> inline uint8 deserialize<uint8>(UEReader& r) { return r.read_u8(); }
	template<> inline uint16 deserialize<uint16>(UEReader& r) { return r.read_u16(); }
	template<> inline uint32 deserialize<uint32>(UEReader& r) { return r.read_u32(); }
	template<> inline uint64 deserialize<uint64>(UEReader& r) { return r.read_u64(); }
	template<> inline int8 deserialize<int8>(UEReader& r) { return r.read_i8(); }
	template<> inline int16 deserialize<int16>(UEReader& r) { return r.read_i16(); }
	template<> inline int32 deserialize<int32>(UEReader& r) { return r.read_i32(); }
	template<> inline int64 deserialize<int64>(UEReader& r) { return r.read_i64(); }
	template<> inline float deserialize<float>(UEReader& r) { return r.read_f32(); }
	template<> inline double deserialize<double>(UEReader& r) { return r.read_f64(); }

	/** @} */ // end of PrimitiveDeserialization group

	// =============================================================================
	// UE Large Integer Type Serialization
	// =============================================================================

	/**
	 * @defgroup UELargeIntegerSerialization UE Large Integer Type Serialization
	 * @brief Serialization for FSpacetimeDB U/Int 128/256 types
	 * @{
	 */

	 /** Serialize FSpacetimeDBUInt128 as two 64-bit integers, little-endian. */
	inline void serialize(UEWriter& w, const FSpacetimeDBUInt128& value)
	{
		w.write_u64(value.GetLower());
		w.write_u64(value.GetUpper());
	}

	/** Deserialize FSpacetimeDBUInt128. */
	template<> inline FSpacetimeDBUInt128 deserialize<FSpacetimeDBUInt128>(UEReader& r)
	{
		uint64 Lower = r.read_u64();
		uint64 Upper = r.read_u64();
		return FSpacetimeDBUInt128(Upper, Lower);
	}

	/** Serialize FSpacetimeDBInt128 as two 64-bit integers, little-endian. */
	inline void serialize(UEWriter& w, const FSpacetimeDBInt128& value)
	{
		w.write_u64(value.GetLower());
		w.write_u64(value.GetUpper());
	}

	/** Deserialize FSpacetimeDBInt128. */
	template<> inline FSpacetimeDBInt128 deserialize<FSpacetimeDBInt128>(UEReader& r)
	{
		uint64 Lower = r.read_u64();
		uint64 Upper = r.read_u64();
		return FSpacetimeDBInt128(Upper, Lower);
	}

	/** Serialize FSpacetimeDBUInt256 as four 64-bit integers, little-endian. */
	inline void serialize(UEWriter& w, const FSpacetimeDBUInt256& value)
	{
		serialize(w, value.GetLower()); // This serializes the lower 128 bits
		serialize(w, value.GetUpper()); // This serializes the upper 128 bits
	}

	/** Deserialize FSpacetimeDBUInt256. */
	template<> inline FSpacetimeDBUInt256 deserialize<FSpacetimeDBUInt256>(UEReader& r)
	{
		FSpacetimeDBUInt128 Lower = deserialize<FSpacetimeDBUInt128>(r);
		FSpacetimeDBUInt128 Upper = deserialize<FSpacetimeDBUInt128>(r);
		return FSpacetimeDBUInt256(Upper, Lower);
	}

	/** Serialize FSpacetimeDBInt256 as four 64-bit integers, little-endian. */
	inline void serialize(UEWriter& w, const FSpacetimeDBInt256& value)
	{
		serialize(w, value.GetLower());
		serialize(w, value.GetUpper());
	}

	/** Deserialize FSpacetimeDBInt256. */
	template<> inline FSpacetimeDBInt256 deserialize<FSpacetimeDBInt256>(UEReader& r)
	{
		FSpacetimeDBUInt128 Lower = deserialize<FSpacetimeDBUInt128>(r);
		FSpacetimeDBUInt128 Upper = deserialize<FSpacetimeDBUInt128>(r);
		return FSpacetimeDBInt256(Upper, Lower);
	}

	/** @} */ // end of UELargeIntegerSerialization group

	// =============================================================================
	// UE Object Pointer Serialization
	// =============================================================================

	/**
	 * @defgroup UEObjectPtrTypes Unreal Engine Object Pointer Types
	 * @brief Serialization for raw and smart UObject pointers
	 * @{
	 */

	 /** Serialize a TObjectPtr<UObject> by dereferencing and dispatching to the object serializer. */
	template<typename T>
	inline void serialize(UEWriter& w, const TObjectPtr<T>& ObjPtr)
	{
		serialize(w, *ObjPtr);
	}

	/** Serialize a raw UObject* pointer by dereferencing and dispatching to the object serializer.*/
	template<typename T>
	inline void serialize(UEWriter& w, T* const& Ptr)
	{
		if (!Ptr) { ensureMsgf(false, TEXT("Cannot serialize null pointer")); return; }
		serialize(w, *Ptr);
	}

	/** Convenience wrapper to deserialize any TObjectPtr<UObject> from a byte array. */
	template<typename T>
	TObjectPtr<T> DeserializePtr(const TArray<uint8>& Bytes)
	{
		return Deserialize<TObjectPtr<T>>(Bytes);
	}

	/** @} */ // end of UEObjectPtrTypes group

	// =============================================================================
	// UE String Type Serialization
	// =============================================================================

	/**
	 * @defgroup UEStringTypes Unreal Engine String Types
	 * @brief Serialization for FString and FName
	 * @{
	 */

	 /** Serialize FString as UTF-8 */
	inline void serialize(UEWriter& w, const FString& str) {
		w.write_string(str);
	}

	/** Deserialize FString from UTF-8 */
	template<> inline FString deserialize<FString>(UEReader& r) {
		return r.read_string();
	}

	/** Serialize FName (converts to FString internally) */
	inline void serialize(UEWriter& w, const FName& name) {
		serialize(w, name.ToString());
	}

	/** Deserialize FName (converts from FString internally) */
	template<> inline FName deserialize<FName>(UEReader& r) {
		return FName(deserialize<FString>(r));
	}

	/** @} */ // end of UEStringTypes group

	// =============================================================================
	// Container Type Helpers
	// =============================================================================

	/**
	 * @defgroup ContainerHelpers Container Type Helper Structures
	 * @brief Internal helpers for container serialization
	 * @{
	 */

	 /**
	  * @brief Helper trait to detect TArray types at compile time
	  */
	template<typename T>
	struct is_tarray_check : std::false_type {};

	template<typename U>
	struct is_tarray_check<TArray<U>> : std::true_type {};

	/**
	 * @brief Helper for specialized deserialization of complex types
	 *
	 * This helper is used to handle deserialization of nested container types
	 * like TArray<TArray<T>> or TOptional<T>.
	 */
	template<typename T>
	struct DeserializeHelper {
		static T deserialize(UEReader& r);
	};

	/** Specialization for TArray */
	template<typename T>
	struct DeserializeHelper<TArray<T>> {
		static TArray<T> deserialize(UEReader& r) {
			return r.read_array<T>();
		}
	};

	/** @} */ // end of ContainerHelpers group

	// =============================================================================
	// Enum Serialization Support
	// =============================================================================

	/**
	 * @defgroup EnumSerialization Enum Serialization
	 * @brief Support for serializing enums as their underlying type
	 * @{
	 */

	 /**
	  * @brief Serialize enum as its underlying type
	  *
	  * SpacetimeDB enums are serialized as their underlying integer type.
	  * This template handles any enum class automatically.
	  */

	 template <typename Enum>
		 requires std::is_enum_v<Enum>
	 inline void serialize(UEWriter& w, const Enum& value)
	 {
		 using Underlying = std::underlying_type_t<Enum>;
		 serialize(w, static_cast<Underlying>(value));
	 }

	 /**
	  * @brief Deserialize enum from its underlying type
	  *
	  * Note: This needs explicit specialization for each enum type to avoid
	  * ambiguity with the generic template. Use the UE_SPACETIMEDB_ENUM macro.
	  */

	template <typename Enum>
		requires std::is_enum_v<Enum>
	inline Enum deserialize(UEReader& r)
	{
		using Underlying = std::underlying_type_t<Enum>;
		return static_cast<Enum>(deserialize<Underlying>(r));
	}

	/** @} */ // end of EnumSerialization group

	// =============================================================================
	// TArray Serialization
	// =============================================================================

	/**
	 * @defgroup TArraySerialization TArray Container Serialization
	 * @brief Serialization support for TArray containers
	 * @{
	 */

	 /**
	  * Serialize a TArray with length prefix
	  * @tparam T Element type
	  * @param w Writer instance
	  * @param arr Array to serialize
	  */
	template<typename T>
	void serialize(UEWriter& w, const TArray<T>& arr) {
		w.write_array(arr);
	}

	/**
	 * Helper function to deserialize TArray
	 * @tparam T Element type
	 * @param r Reader instance
	 * @return Deserialized array
	 */
	template<typename T>
	TArray<T> deserialize_array(UEReader& r) {
		return r.read_array<T>();
	}

	/** @} */ // end of TArraySerialization group

	// =============================================================================
	// TOptional Serialization
	// =============================================================================

	/**
	 * @Note. TOptional not compatable with Blueprints, therefore we use a custom optional approach. But we keep this in case blueprints support is added in the future.
	 * @defgroup TOptionalSerialization TOptional Container Serialization
	 * @brief Serialization support for TOptional containers
	 * @{
	 */

	 /**
	  * Serialize TOptional with tag byte
	  * @tparam T Value type
	  * @param w Writer instance
	  * @param opt Optional value to serialize
	  * @note Uses tag 0 for Some, tag 1 for None
	  */
	template<typename T>
	void serialize(UEWriter& w, const TOptional<T>& opt) {
		if (opt.IsSet()) {
			w.write_u8(0); // Some tag
			serialize(w, opt.GetValue());
		}
		else {
			w.write_u8(1); // None tag
		}
	}

	/**
	 * Helper function to deserialize TOptional
	 * @tparam T Value type
	 * @param r Reader instance
	 * @return Deserialized optional
	 */
	template<typename T>
	TOptional<T> deserialize_optional(UEReader& r) {
		uint8_t tag = r.read_u8();
		if (tag == 0) {
			return TOptional<T>(deserialize<T>(r));
		}
		else if (tag == 1) {
			return TOptional<T>();
		}
		else {
			ensureMsgf(false, TEXT("Invalid optional tag: %d"), tag);
			return TOptional<T>();
		}
	}

	/** Specialization for TOptional */
	template<typename T>
	struct DeserializeHelper<TOptional<T>> {
		static TOptional<T> deserialize(UEReader& r) {
			return deserialize_optional<T>(r);
		}
	};

	/** @} */ // end of TOptionalSerialization group

	// =============================================================================
	// Custom Optional struct Serialization
	// =============================================================================
	/**
	 * @brief Helper macro to generate serialization for Blueprint compatible optionals
	 * @{
	 *
	 * This macro creates serialize/deserialize specializations for a struct that
	 * mimics the behaviour of TOptional by exposing a value field and a boolean
	 * flag indicating if the value is set.
	 *
	 * @param StructType  Name of the optional wrapper struct
	 * @param ValueField  Name of the value member inside the struct
	 * @param IsSetField  Name of the boolean member that signals if the value is present
	 *
	 * Example usage:
	 * @code
	 * USTRUCT(BlueprintType)
	 * struct FMyIntOptional {
	 *     int32 Value;
	 *     bool  bIsSet;
	 * };
	 *
	 * namespace UE::SpacetimeDB {
	 *     UE_SPACETIMEDB_OPTIONAL(FMyIntOptional, Value, bIsSet);
	 * }
	 * @endcode
	 */
#define UE_SPACETIMEDB_OPTIONAL(StructType, IsSetField, ValueField) \
	template<> inline void serialize<StructType>(UEWriter& w, const StructType& value) { \
		if (value.IsSetField) { \
			w.write_u8(0); \
			serialize(w, value.ValueField); \
		} else { \
			w.write_u8(1); \
		} \
	} \
	template<> inline StructType deserialize<StructType>(UEReader& r) { \
		StructType result; \
		uint8_t tag = r.read_u8(); \
		if (tag == 0) { \
			result.IsSetField = true; \
			result.ValueField = deserialize<decltype(result.ValueField)>(r); \
		} else if (tag == 1) { \
			result.IsSetField = false; \
		} else { \
			ensureMsgf(false, TEXT("Invalid optional tag: %d"), tag); \
			return StructType(); \
		} \
		return result; \
	}

	 /** @} */ // end of CustomOptionalStructSerialization group


	// =============================================================================
	// UE Utility Type Serialization
	// =============================================================================

	/** Serialize FDateTime as ticks (int64) */
	inline void serialize(UEWriter& w, const FDateTime& dt) {
		w.write_i64(dt.GetTicks());
	}

	/** Deserialize FDateTime from ticks */
	template<> inline FDateTime deserialize<FDateTime>(UE::SpacetimeDB::UEReader& r) {
		return FDateTime(r.read_i64());
	}

	/** Serialize FTimespan as ticks (int64) */
	inline void serialize(UEWriter& w, const FTimespan& ts) {
		w.write_i64(ts.GetTicks());
	}

	/** Deserialize FTimespan from ticks */
	template<> inline FTimespan deserialize<FTimespan>(UE::SpacetimeDB::UEReader& r) {
		return FTimespan(r.read_i64());
	}

	/** @} */ // end of UEUtilityTypes group

	// =============================================================================
	// Template Method Implementations
	// =============================================================================

	/**
	 * @brief Implementation of generic TArray serialization
	 *
	 * Must be defined after serialize() declarations to allow proper template
	 * instantiation for element types.
	 */
	template<typename T>
	void UEWriter::write_array(const TArray<T>& arr) {
		// Write count as 32-bit length prefix
		core_writer.write_u32_le(static_cast<uint32_t>(arr.Num()));

		// Serialize each element
		for (const auto& item : arr) {
			serialize(*this, item);
		}
	}

	/**
	 * @brief Implementation of generic TArray deserialization
	 *
	 * Handles nested arrays properly by using DeserializeHelper for
	 * complex types like TArray<TArray<T>>.
	 */
	template<typename T>
	TArray<T> UEReader::read_array() {
		// Read count from 32-bit length prefix
		uint32_t count = core_reader.read_u32_le();

		// Pre-allocate array
		TArray<T> result;
		result.Reserve(count);

		// Deserialize each element
		for (uint32_t i = 0; i < count; ++i) {
			// Use DeserializeHelper for nested containers, direct deserialize for primitives
			if constexpr (is_tarray_check<T>::value) {
				result.Add(DeserializeHelper<T>::deserialize(*this));
			}
			else {
				result.Add(deserialize<T>(*this));
			}
		}
		return result;
	}

	// =============================================================================
	// High-Level Serialization API
	// =============================================================================

	/**
	 * @defgroup HighLevelAPI High-Level Serialization API
	 * @brief Simple functions for serializing/deserializing UE types
	 * @{
	 */

	 /**
	  * @brief Type trait to detect TArray types
	  */
	template<typename T>
	struct is_tarray : std::false_type {};

	template<typename T>
	struct is_tarray<TArray<T>> : std::true_type {};

	template<typename T>
	inline constexpr bool is_tarray_v = is_tarray<T>::value;

	/**
	 * @brief Type trait to detect TOptional types
	 */
	template<typename T>
	struct is_toptional : std::false_type {};

	template<typename T>
	struct is_toptional<TOptional<T>> : std::true_type {};

	template<typename T>
	inline constexpr bool is_toptional_v = is_toptional<T>::value;

	/**
	 * Serialize any supported UE type to a byte array
	 * @tparam T Type to serialize (automatically deduced)
	 * @param value The value to serialize
	 * @return TArray<uint8> containing the serialized BSATN data
	 *
	 * @code
	 * FVector position(100, 200, 300);
	 * TArray<uint8> data = UE::SpacetimeDB::Serialize(position);
	 * @endcode
	 */
	template<typename T>
	TArray<uint8> Serialize(const T& value) {
		UEWriter writer;
		serialize(writer, value);
		return std::move(writer).take_buffer();
	}

	/**
	 * Deserialize a byte array to a UE type
	 * @tparam T Type to deserialize to (must be explicitly specified)
	 * @param data The byte array containing BSATN data
	 * @return The deserialized value
	 *
	 * @code
	 * TArray<uint8> data = GetSerializedData();
	 * FString Name = UE::SpacetimeDB::Deserialize<FString>(data);
	 * @endcode
	 */
	template<typename T>
	T Deserialize(const TArray<uint8>& data) {

		UEReader reader(data);

		// Use appropriate deserialization path based on type

		if constexpr (is_tarray_v<T>) {
			return DeserializeHelper<T>::deserialize(reader);
		}
		else if constexpr (is_toptional_v<T>) {
			return DeserializeHelper<T>::deserialize(reader);
		}
		else {
			return deserialize<T>(reader);
		}
	}

	/** @} */ // end of HighLevelAPI group

	// =============================================================================
	// Container Type Specialization Helpers
	// =============================================================================

	/**
	 * @defgroup ContainerMacros Container Type Helper Macros
	 * @brief Macros to enable serialization of custom types in containers
	 * @{
	 */

	 /**
	  * @brief Helper macro to generate deserialize specialization for TArray<T>
	  *
	  * Use this macro when you have structs with TArray fields of custom types.
	  * Place it in the UE::SpacetimeDB namespace before using UE_SPACETIMEDB_STRUCT.
	  *
	  * @code
	  * namespace UE::SpacetimeDB {
	  *     UE_SPACETIMEDB_ENABLE_TARRAY(FMyCustomType)
	  *     UE_SPACETIMEDB_STRUCT(MyStruct, field1, myArrayField)
	  * }
	  * @endcode
	  */
#define UE_SPACETIMEDB_ENABLE_TARRAY(ElementType) \
	template<> inline TArray<ElementType> deserialize<TArray<ElementType>>(UEReader& r) { \
		return r.read_array<ElementType>(); \
	}

	  // =============================================================================
	 // TOptional Serialization
	 // =============================================================================

	  /**
	   * @brief Helper macro to generate deserialize specialization for TOptional<T>
	   *
	   * Use this macro when you have structs with TOptional fields of custom types.
	   * 
	   * @Note: We are not using TOptional directly becouse it is not compatable wiht blueprints.
	   * This macro is kept for future compatibility if things change on the Engine side.
	   *
	   * @code
	   * namespace UE::SpacetimeDB {
	   *     UE_SPACETIMEDB_ENABLE_TOPTIONAL(FMyCustomType)
	   *     UE_SPACETIMEDB_STRUCT(MyStruct, field1, myOptionalField)
	   * }
	   * @endcode
	   */
#define UE_SPACETIMEDB_ENABLE_TOPTIONAL(ElementType) \
	template<> inline TOptional<ElementType> deserialize<TOptional<ElementType>>(UEReader& r) { \
		return deserialize_optional<ElementType>(r); \
	}



	   /** @} */ // end of TOptionalSerialization group

	   // =============================================================================
   // Tagged Enum (Enum + TVariant) Macro
   // =============================================================================

   // Reuse the existing FOR_EACH machinery already defined above in this file:
   //   UE_FOR_EACH_PAIR(M, EnumTok, FieldTok, ...)
   // It calls M(EnumTok, FieldTok, TagTok, TypeTok) for every (Tag, Type) pair.

   // Single-pair emitters for TVariant-based structs
#define UE_WRITE_CASE_TVARIANT(EnumTok, FieldTok, TagTok, TypeTok)                     \
	case EnumTok::TagTok: {                                                            \
		/* TVariant::Get<T>() returns the value (or ref for ref-qualified variants) */ \
		const auto& _val = v.FieldTok.Get<TypeTok>();                                  \
		serialize(w, _val);                                                            \
		break;                                                                         \
	}

#define UE_READ_CASE_TVARIANT(EnumTok, FieldTok, TagTok, TypeTok)                      \
	case EnumTok::TagTok: {                                                            \
		auto _tmp = deserialize<TypeTok>(r);                                           \
		out.FieldTok.Set<TypeTok>(_tmp);                                               \
		break;                                                                         \
	}

/**
 * UE_SPACETIMEDB_TAGGED_ENUM
 * Generates serialize/deserialize for a (USTRUCT-like) tagged enum that stores
 * its payload in a TVariant field.
 *
 * @param Struct    The USTRUCT type (value type, not UObject)
 * @param Enum      The UENUM tag type (e.g., EMyTag)
 * @param Field     The TVariant field name in Struct (e.g., MessageData)
 * @param ...       Repeated (Tag, Type) pairs, e.g.:  Foo, int32, Bar, FString, ...
 *
 * Example:
 *   UE_SPACETIMEDB_TAGGED_ENUM(
 *       FMyTagged, EMyTag, Payload,
 *       Foo, int32,
 *       Bar, FString)
 */
#define UE_SPACETIMEDB_TAGGED_ENUM(Struct, Enum, Field, ...)                           \
	/* ---- serialize ---------------------------------------------------- */          \
	template<> inline void serialize<Struct>(UEWriter& w, const Struct& v)             \
	{                                                                                  \
		w.write_u8(static_cast<uint8>(v.Tag));                                         \
		switch (v.Tag) {                                                               \
			UE_FOR_EACH_PAIR(UE_WRITE_CASE_TVARIANT, Enum, Field, __VA_ARGS__)         \
		default:                                                                        \
			ensureMsgf(false, TEXT("Unknown tag in %s::serialize"), TEXT(#Struct));    \
			break;                                                                      \
		}                                                                              \
	}                                                                                  \
	/* ---- deserialize -------------------------------------------------- */          \
	template<> inline Struct deserialize<Struct>(UEReader& r)                          \
	{                                                                                  \
		Struct out{};                                                                  \
		const auto Tag = static_cast<Enum>(r.read_u8());                               \
		out.Tag = Tag;                                                                 \
		switch (Tag) {                                                                 \
			UE_FOR_EACH_PAIR(UE_READ_CASE_TVARIANT, Enum, Field, __VA_ARGS__)          \
		default:                                                                        \
			ensureMsgf(false, TEXT("Unknown tag in %s::deserialize"), TEXT(#Struct));  \
			/* out left default-initialized */                                         \
			break;                                                                      \
		}                                                                              \
		return out;                                                                    \
	}



// ────────────────────────────────────────────────────────────────────────────
// 0. Force extra rescans so MSVC finishes the recursion
// ────────────────────────────────────────────────────────────────────────────
#define UE_EVAL0(...) __VA_ARGS__
#define UE_EVAL1(...) UE_EVAL0(UE_EVAL0(__VA_ARGS__))
#define UE_EVAL2(...) UE_EVAL1(UE_EVAL1(__VA_ARGS__))
#define UE_EVAL3(...) UE_EVAL2(UE_EVAL2(__VA_ARGS__))
#define UE_EVAL4(...) UE_EVAL3(UE_EVAL3(__VA_ARGS__))
#define UE_EVAL5(...) UE_EVAL4(UE_EVAL4(__VA_ARGS__))
#define UE_EVAL(...)  UE_EVAL5(__VA_ARGS__) // 32 rescans → ~64 tokens (~32 pairs)

#define UE_PARENS ()                                 // helper

// ────────────────────────────────────────────────────────────────────────────
// 1. FOR_EACH_2 that also carries Enum-token and Field-token
//    M(EnumTok, FieldTok, TagTok, TypeTok)
// ────────────────────────────────────────────────────────────────────────────
#define UE_FOR_EACH_PAIR(M, EnumTok, FieldTok, ...)                                    \
		__VA_OPT__(UE_EVAL(UE_FOR_EACH_HELPER(M, EnumTok, FieldTok, __VA_ARGS__)))

#define UE_FOR_EACH_HELPER(M, EnumTok, FieldTok, Tag, Type, ...)                       \
		M(EnumTok, FieldTok, Tag, Type)                                                \
		__VA_OPT__(UE_FOR_EACH_AGAIN UE_PARENS (M, EnumTok, FieldTok, __VA_ARGS__))

#define UE_FOR_EACH_AGAIN()  UE_FOR_EACH_HELPER

// ────────────────────────────────────────────────────────────────────────────
// 2. Emitters for a single (Tag,Type) pair
// ────────────────────────────────────────────────────────────────────────────
#define UE_WRITE_CASE(EnumTok, FieldTok, TagTok, TypeTok)                              \
		case EnumTok::TagTok:                                                          \
			serialize(w, *v.FieldTok.GetPtr<TypeTok>());              \
			break;

#define UE_READ_CASE(EnumTok, FieldTok, TagTok, TypeTok)                               \
		case EnumTok::TagTok: {                                                        \
			auto tmp = deserialize<TypeTok>(r);                       \
			obj->FieldTok.InitializeAs<TypeTok>(tmp);                                  \
			break; }

// ────────────────────────────────────────────────────────────────────────────
// 3. Master macro
// ────────────────────────────────────────────────────────────────────────────
#define UE_SPACETIMEDB_TAGGED_VARIANT(Class, Enum, Field, ...)                         \
	/* ---- serialize ---------------------------------------------------- */          \
	template<> inline void serialize<Class>(                                          \
		UEWriter& w, const Class& v)                                                   \
	{                                                                                  \
		w.write_u8(static_cast<uint8>(v.Tag));                                         \
		switch (v.Tag)                                                                 \
		{                                                                              \
			UE_FOR_EACH_PAIR(UE_WRITE_CASE, Enum, Field, __VA_ARGS__)                  \
		default: ensureMsgf(false, TEXT("Unknown tag"));                               \
		}                                                                              \
	}                                                                                  \
	/* ---- deserialize -------------------------------------------------- */          \
	template<> inline TObjectPtr<Class>                                                \
	deserialize<TObjectPtr<Class>>(UEReader& r)                                        \
	{                                                                                  \
		const auto Tag = static_cast<Enum>(r.read_u8());                               \
		Class* obj = NewObject<Class>();                                               \
		obj->Tag = Tag;                                                                \
		switch (Tag)                                                                   \
		{                                                                              \
			UE_FOR_EACH_PAIR(UE_READ_CASE, Enum, Field, __VA_ARGS__)                  \
		default:                                                                       \
			ensureMsgf(false, TEXT("Unknown tag in %s"), TEXT(#Class));               \
			return nullptr;                                                            \
		}                                                                              \
		return obj;                                                                    \
	}


// =============================================================================
// Struct Serialization Macro
// =============================================================================

/**
 * @defgroup StructMacros Struct Serialization Macros
 * @brief Macros for enabling serialization of custom structs
 * @{
 */

 /**
  * @brief Enable BSATN serialization for a custom struct
  *
  * This macro generates serialize() and deserialize() functions for a struct
  * by serializing each field in order.
  *
  * @param StructName The name of the struct
  * @param ... Comma-separated list of field names (up to 10 fields)
  *
  * Example usage:
  * @code
  * struct FMyStruct {
  *     FString Name;
  *     int32 Value;
  *     FVector Position;
  * };
  *
  * // Enable serialization (place in UE::SpacetimeDB namespace)
  * namespace UE::SpacetimeDB {
  *     UE_SPACETIMEDB_STRUCT(FMyStruct, Name, Value, Position)
  * }
  * @endcode
  *
  * @note Fields are serialized in the order specified
  * @note The struct must have a default constructor
  */
#define UE_SPACETIMEDB_STRUCT(StructName, ...) \
	template<> inline void serialize<StructName>(UEWriter& w, const StructName& value) { \
		UE_SPACETIMEDB_SERIALIZE_FIELDS(w, value, __VA_ARGS__) \
	} \
	template<> inline StructName deserialize<StructName>(UEReader& r) { \
		StructName result; \
		UE_SPACETIMEDB_DESERIALIZE_FIELDS(r, result, __VA_ARGS__) \
		return result; \
	}

  // -----------------------------------------------------------------------------
  // Internal Helper Macros - Not for Direct Use
  // -----------------------------------------------------------------------------

  /** @cond INTERNAL */

  // Field serialization/deserialization macros
#define UE_SPACETIMEDB_SERIALIZE_FIELD(obj, writer, field) \
	serialize(writer, obj.field);

#define UE_SPACETIMEDB_DESERIALIZE_FIELD(obj, reader, field) \
	obj.field = deserialize<decltype(obj.field)>(reader);

// Expand macros for each field
#define UE_SPACETIMEDB_SERIALIZE_FIELDS(writer, obj, ...) \
	UE_SPACETIMEDB_FOR_EACH_ARG(UE_SPACETIMEDB_SERIALIZE_FIELD, obj, writer, __VA_ARGS__)

#define UE_SPACETIMEDB_DESERIALIZE_FIELDS(reader, obj, ...) \
	UE_SPACETIMEDB_FOR_EACH_ARG(UE_SPACETIMEDB_DESERIALIZE_FIELD, obj, reader, __VA_ARGS__)

// Macro utilities for variadic expansion (supports up to 10 fields)
#define UE_SPACETIMEDB_GET_MACRO( \
	_1, _2, _3, _4, _5, _6, _7, _8, _9, _10, \
	_11, _12, _13, _14, _15, _16, _17, _18, _19, _20, \
	_21, _22, _23, _24, _25, _26, _27, _28, _29, _30, NAME, ...) NAME


#define UE_SPACETIMEDB_FOR_EACH_ARG(MACRO, obj, extra, ...) \
	UE_SPACETIMEDB_GET_MACRO(__VA_ARGS__, \
		UE_SPACETIMEDB_FE_30, UE_SPACETIMEDB_FE_29, UE_SPACETIMEDB_FE_28, UE_SPACETIMEDB_FE_27, UE_SPACETIMEDB_FE_26, \
		UE_SPACETIMEDB_FE_25, UE_SPACETIMEDB_FE_24, UE_SPACETIMEDB_FE_23, UE_SPACETIMEDB_FE_22, UE_SPACETIMEDB_FE_21, \
		UE_SPACETIMEDB_FE_20, UE_SPACETIMEDB_FE_19, UE_SPACETIMEDB_FE_18, UE_SPACETIMEDB_FE_17, UE_SPACETIMEDB_FE_16, \
		UE_SPACETIMEDB_FE_15, UE_SPACETIMEDB_FE_14, UE_SPACETIMEDB_FE_13, UE_SPACETIMEDB_FE_12, UE_SPACETIMEDB_FE_11, \
		UE_SPACETIMEDB_FE_10, UE_SPACETIMEDB_FE_9, UE_SPACETIMEDB_FE_8, UE_SPACETIMEDB_FE_7, UE_SPACETIMEDB_FE_6, \
		UE_SPACETIMEDB_FE_5, UE_SPACETIMEDB_FE_4, UE_SPACETIMEDB_FE_3, UE_SPACETIMEDB_FE_2, UE_SPACETIMEDB_FE_1) \
	(MACRO, obj, extra, __VA_ARGS__)


// Field expansion macros (1-30 fields)
#define UE_SPACETIMEDB_FE_1(MACRO, obj, extra, X) MACRO(obj, extra, X)
#define UE_SPACETIMEDB_FE_2(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_1(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_3(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_2(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_4(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_3(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_5(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_4(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_6(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_5(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_7(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_6(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_8(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_7(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_9(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_8(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_10(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_9(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_11(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_10(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_12(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_11(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_13(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_12(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_14(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_13(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_15(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_14(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_16(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_15(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_17(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_16(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_18(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_17(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_19(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_18(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_20(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_19(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_21(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_20(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_22(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_21(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_23(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_22(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_24(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_23(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_25(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_24(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_26(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_25(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_27(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_26(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_28(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_27(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_29(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_28(MACRO, obj, extra, __VA_ARGS__)
#define UE_SPACETIMEDB_FE_30(MACRO, obj, extra, X, ...) MACRO(obj, extra, X) UE_SPACETIMEDB_FE_29(MACRO, obj, extra, __VA_ARGS__)

/** @endcond */ // end of INTERNAL

// =============================================================================
// Empty Struct Serialization Macro
// =============================================================================

/**
 * @cond EmptyStructMacros Empty-Struct Serialization Macros
 * @brief Helpers for treating UE structs with **no data members** as
 *        0-byte “unit” types in BSATN.
 * @{
 */

 /**
  * @brief Enable BSATN serialization for an *empty* struct.
  *
  * Expands to **no-op** `serialize` / `deserialize` specializations, so the
  * struct neither writes nor reads any bytes.
  *
  * @param StructName  The struct type to register.
  *
  * ### Example
  * ```cpp
  * USTRUCT(BlueprintType)
  * struct FIdentityConnectedArgs
  * {
  *     GENERATED_BODY()
  * };
  *
  * namespace UE::SpacetimeDB
  * {
  *     UE_SPACETIMEDB_STRUCT_EMPTY(FIdentityConnectedArgs)
  * }
  * ```
  */
#define UE_SPACETIMEDB_STRUCT_EMPTY(StructName)                                 \
	template<> inline void serialize<StructName>(UEWriter&,    \
												 const StructName&)             \
	{ /* intentionally empty */ }                                               \
																				\
	template<> inline StructName deserialize<StructName>(UEReader&) \
	{ return StructName(); }

  /** @endcond */ // EmptyStructMacros

/** @} */ // end of StructMacros group

// =============================================================================
// Enum Serialization Macro
// =============================================================================

/**
 * @defgroup EnumMacros Enum Serialization Macros
 * @brief Macros for enabling serialization of enum types
 * @{
 */

 /**
  * @brief Enable BSATN serialization for an enum type
  *
  * This macro generates a deserialize specialization for enum types.
  * The serialize function is already handled by the generic template.
  *
  * @param EnumType The enum type to enable serialization for
  *
  * Example usage:
  * @code
  * enum class EGameState : uint8_t {
  *     Lobby = 0,
  *     InGame = 1,
  *     GameOver = 2
  * };
  *
  * // Enable serialization
  * namespace UE::SpacetimeDB {
  *     UE_SPACETIMEDB_ENUM(EGameState)
  * }
  * @endcode
  */
#define UE_SPACETIMEDB_ENUM(EnumType) \
	template<> inline EnumType deserialize<EnumType>(UEReader& r) { \
		return deserialize_enum<EnumType>(r); \
	}

  /** @} */ // end of EnumMacros group

  // =============================================================================
  // Common Container Specializations
  // =============================================================================

  /**
   * @defgroup CommonSpecializations Common Container Specializations
   * @brief Pre-defined specializations for frequently used container types
   * @{
   */

   // String containers
	UE_SPACETIMEDB_ENABLE_TARRAY(FString)
	UE_SPACETIMEDB_ENABLE_TARRAY(FName)
	// Primitive containers
	UE_SPACETIMEDB_ENABLE_TARRAY(int8)
	UE_SPACETIMEDB_ENABLE_TARRAY(uint8)
	UE_SPACETIMEDB_ENABLE_TARRAY(int16)
	UE_SPACETIMEDB_ENABLE_TARRAY(uint16)
	UE_SPACETIMEDB_ENABLE_TARRAY(int32)
	UE_SPACETIMEDB_ENABLE_TARRAY(uint32)
	UE_SPACETIMEDB_ENABLE_TARRAY(int64)
	UE_SPACETIMEDB_ENABLE_TARRAY(uint64)

	UE_SPACETIMEDB_ENABLE_TARRAY(float)
	UE_SPACETIMEDB_ENABLE_TARRAY(double)
	
	UE_SPACETIMEDB_ENABLE_TARRAY(bool)

	// Large integer type containers
	UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBUInt128)
	UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBUInt256)
	UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBInt128)
	UE_SPACETIMEDB_ENABLE_TARRAY(FSpacetimeDBInt256)
	

	/** @} */ // end of CommonSpecializations group

} // namespace UE::SpacetimeDB