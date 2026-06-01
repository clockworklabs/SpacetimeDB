#ifndef SPACETIMEDB_HTTP_WIRE_H
#define SPACETIMEDB_HTTP_WIRE_H

#pragma once

#include <string>
#include <vector>
#include <optional>
#include <cstdint>
#include "spacetimedb/bsatn/bsatn.h"
#include "spacetimedb/bsatn/traits.h"
#include "spacetimedb/bsatn/time_duration.h"

/**
 * @file http_wire.h
 * @brief BSATN wire format types for HTTP requests/responses
 *
 * These types mirror the Rust types in `spacetimedb_lib::http` and are used for
 * BSATN encoding/decoding when communicating with the SpacetimeDB host.
 *
 * CRITICAL: Field order MUST match Rust exactly for BSATN compatibility!
 *
 * These types are internal implementation details. User code should use the types
 * in http.h instead. Conversion functions in http_conversions.h handle the mapping.
 *
 * @warning Do NOT change the field order or layout of these types without coordinating
 *          with the Rust side. Breaking BSATN compatibility will cause runtime failures.
 *
 * @ingroup sdk_internal
 */

namespace SpacetimeDB {
namespace wire {

/**
 * @brief Wire format for HTTP method
 *
 * Matches Rust: `spacetimedb_lib::http::Method`
 *
 * BSATN enum representation:
 * - Standard methods (Get, Head, Post, etc.) are represented as unit variants (no payload)
 * - Extension(String) is represented as a variant with a String payload
 */
struct HttpMethod {
    enum class Tag : uint8_t {
        Get = 0,
        Head = 1,
        Post = 2,
        Put = 3,
        Delete = 4,
        Connect = 5,
        Options = 6,
        Trace = 7,
        Patch = 8,
        Extension = 9,
    };

    Tag tag;
    std::string extension; // Only valid when tag == Extension
};

/**
 * @brief Wire format for HTTP version
 *
 * Matches Rust: `spacetimedb_lib::http::Version`
 *
 * BSATN enum representation (unit variants only):
 * - Http09 = 0
 * - Http10 = 1
 * - Http11 = 2
 * - Http2 = 3
 * - Http3 = 4
 */
struct HttpVersion {
    enum class Tag : uint8_t {
        Http09 = 0,
        Http10 = 1,
        Http11 = 2,
        Http2 = 3,
        Http3 = 4,
    };

    Tag tag;
};

/**
 * @brief Wire format for a single HTTP header name/value pair
 *
 * Matches Rust: `spacetimedb_lib::http::HttpHeaderPair`
 *
 * Field order: name, value (MUST match Rust!)
 *
 * Note: The `is_sensitive` flag from the user-facing HttpHeader type is NOT transmitted.
 * It's a local-only hint and is not part of the wire format.
 */
struct HttpHeaderPair {
    std::string name;           // Field 0: Header name (valid HTTP header name)
    std::vector<uint8_t> value; // Field 1: Header value bytes
};

/**
 * @brief Wire format for HTTP headers collection
 *
 * Matches Rust: `spacetimedb_lib::http::Headers`
 *
 * Field order: entries (MUST match Rust!)
 *
 * BSATN representation:
 * - Single field `entries` which is a Vec<HttpHeaderPair>
 * - Headers with the same name appear as multiple entries
 */
struct HttpHeaders {
    std::vector<HttpHeaderPair> entries; // Field 0: Array of header pairs
};

/**
 * @brief Wire format for HTTP request
 *
 * Matches Rust: `spacetimedb_lib::http::Request`
 *
 * Field order (CRITICAL - MUST match Rust exactly!):
 * 0. method: HttpMethod
 * 1. headers: HttpHeaders
 * 2. timeout: Option<TimeDuration>
 * 3. uri: String
 * 4. version: HttpVersion
 *
 * Note: The request body is NOT part of this struct. It's passed separately
 * to the host via the ConsumeBytes() mechanism.
 */
struct HttpRequest {
    HttpMethod method;                      // Field 0
    HttpHeaders headers;                    // Field 1
    std::optional<TimeDuration> timeout;    // Field 2
    std::string uri;                        // Field 3
    HttpVersion version;                    // Field 4
};

/**
 * @brief Wire format for HTTP response
 *
 * Matches Rust: `spacetimedb_lib::http::Response`
 *
 * Field order (CRITICAL - MUST match Rust exactly!):
 * 0. headers: HttpHeaders
 * 1. version: HttpVersion
 * 2. code: u16
 *
 * Note: The response body is NOT part of this struct. It's received separately
 * from the host via the ConsumeBytes() mechanism.
 */
struct HttpResponse {
    HttpHeaders headers;  // Field 0
    HttpVersion version;  // Field 1
    uint16_t code;        // Field 2: HTTP status code
};

} // namespace wire
} // namespace SpacetimeDB

// ==================== BSATN Serialization Traits ====================

namespace SpacetimeDB::bsatn {

// Forward declarations for recursive types
template<> struct bsatn_traits<wire::HttpMethod>;
template<> struct bsatn_traits<wire::HttpVersion>;
template<> struct bsatn_traits<wire::HttpHeaderPair>;
template<> struct bsatn_traits<wire::HttpHeaders>;
template<> struct bsatn_traits<wire::HttpRequest>;
template<> struct bsatn_traits<wire::HttpResponse>;

// HttpMethod enum serialization
template<>
struct bsatn_traits<wire::HttpMethod> {
    static void serialize(Writer& writer, const wire::HttpMethod& value) {
        // Encode tag as u8
        writer.write_u8(static_cast<uint8_t>(value.tag));
        
        // If Extension variant, encode the string payload
        if (value.tag == wire::HttpMethod::Tag::Extension) {
            bsatn::serialize(writer, value.extension);
        }
    }

    static wire::HttpMethod deserialize(Reader& reader) {
        wire::HttpMethod result;
        result.tag = static_cast<wire::HttpMethod::Tag>(reader.read_u8());
        
        // If Extension variant, decode the string payload
        if (result.tag == wire::HttpMethod::Tag::Extension) {
            result.extension = bsatn::deserialize<std::string>(reader);
        }
        
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U8();
    }
};

// HttpVersion enum serialization (simple tag-only enum)
template<>
struct bsatn_traits<wire::HttpVersion> {
    static void serialize(Writer& writer, const wire::HttpVersion& value) {
        writer.write_u8(static_cast<uint8_t>(value.tag));
    }

    static wire::HttpVersion deserialize(Reader& reader) {
        wire::HttpVersion result;
        result.tag = static_cast<wire::HttpVersion::Tag>(reader.read_u8());
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        return AlgebraicType::U8();
    }
};

// HttpHeaderPair struct serialization
template<>
struct bsatn_traits<wire::HttpHeaderPair> {
    static void serialize(Writer& writer, const wire::HttpHeaderPair& value) {
        // Field 0: name (String)
        bsatn::serialize(writer, value.name);
        // Field 1: value (Vec<u8>)
        bsatn::serialize(writer, value.value);
    }

    static wire::HttpHeaderPair deserialize(Reader& reader) {
        wire::HttpHeaderPair result;
        // Field 0: name
        result.name = bsatn::deserialize<std::string>(reader);
        // Field 1: value
        result.value = bsatn::deserialize<std::vector<uint8_t>>(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        ProductTypeBuilder builder;
        builder.with_field<std::string>("name");
        builder.with_field<std::vector<uint8_t>>("value");
        return AlgebraicType::make_product(builder.build());
    }
};

// HttpHeaders struct serialization
template<>
struct bsatn_traits<wire::HttpHeaders> {
    static void serialize(Writer& writer, const wire::HttpHeaders& value) {
        // Field 0: entries (Vec<HttpHeaderPair>)
        bsatn::serialize(writer, value.entries);
    }

    static wire::HttpHeaders deserialize(Reader& reader) {
        wire::HttpHeaders result;
        // Field 0: entries
        result.entries = bsatn::deserialize<std::vector<wire::HttpHeaderPair>>(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        ProductTypeBuilder builder;
        builder.with_field<std::vector<wire::HttpHeaderPair>>("entries");
        return AlgebraicType::make_product(builder.build());
    }
};

// HttpRequest struct serialization
template<>
struct bsatn_traits<wire::HttpRequest> {
    static void serialize(Writer& writer, const wire::HttpRequest& value) {
        // Field 0: method
        bsatn::serialize(writer, value.method);
        // Field 1: headers
        bsatn::serialize(writer, value.headers);
        // Field 2: timeout
        bsatn::serialize(writer, value.timeout);
        // Field 3: uri
        bsatn::serialize(writer, value.uri);
        // Field 4: version
        bsatn::serialize(writer, value.version);
    }

    static wire::HttpRequest deserialize(Reader& reader) {
        wire::HttpRequest result;
        // Field 0: method
        result.method = bsatn::deserialize<wire::HttpMethod>(reader);
        // Field 1: headers
        result.headers = bsatn::deserialize<wire::HttpHeaders>(reader);
        // Field 2: timeout
        result.timeout = bsatn::deserialize<std::optional<TimeDuration>>(reader);
        // Field 3: uri
        result.uri = bsatn::deserialize<std::string>(reader);
        // Field 4: version
        result.version = bsatn::deserialize<wire::HttpVersion>(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        ProductTypeBuilder builder;
        builder.with_field<wire::HttpMethod>("method");
        builder.with_field<wire::HttpHeaders>("headers");
        builder.with_field<std::optional<TimeDuration>>("timeout");
        builder.with_field<std::string>("uri");
        builder.with_field<wire::HttpVersion>("version");
        return AlgebraicType::make_product(builder.build());
    }
};

// HttpResponse struct serialization
template<>
struct bsatn_traits<wire::HttpResponse> {
    static void serialize(Writer& writer, const wire::HttpResponse& value) {
        // Field 0: headers
        bsatn::serialize(writer, value.headers);
        // Field 1: version
        bsatn::serialize(writer, value.version);
        // Field 2: code
        bsatn::serialize(writer, value.code);
    }

    static wire::HttpResponse deserialize(Reader& reader) {
        wire::HttpResponse result;
        // Field 0: headers
        result.headers = bsatn::deserialize<wire::HttpHeaders>(reader);
        // Field 1: version
        result.version = bsatn::deserialize<wire::HttpVersion>(reader);
        // Field 2: code
        result.code = bsatn::deserialize<uint16_t>(reader);
        return result;
    }
    
    static AlgebraicType algebraic_type() {
        ProductTypeBuilder builder;
        builder.with_field<wire::HttpHeaders>("headers");
        builder.with_field<wire::HttpVersion>("version");
        builder.with_field<uint16_t>("code");
        return AlgebraicType::make_product(builder.build());
    }
};

} // namespace SpacetimeDB::bsatn

#endif // SPACETIMEDB_HTTP_WIRE_H
