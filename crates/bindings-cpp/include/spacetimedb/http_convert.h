#ifndef SPACETIMEDB_HTTP_CONVERT_H
#define SPACETIMEDB_HTTP_CONVERT_H

#pragma once

#include "spacetimedb/http.h"
#include "spacetimedb/http_wire.h"

/**
 * @file http_convert.h
 * @brief Conversion functions between user-facing HTTP types and BSATN wire types
 *
 * This module provides bidirectional conversion between:
 * - User-facing types (http.h): HttpMethod, HttpVersion, HttpHeader, etc.
 * - Wire types (http_wire.h): wire::HttpMethod, wire::HttpVersion, etc.
 *
 * The wire types are used for BSATN serialization when communicating with the
 * SpacetimeDB host. User code should never interact with wire types directly.
 *
 * Note: The `is_sensitive` flag from HttpHeader is NOT preserved in the wire format.
 * It's a local-only hint and is lost during conversion to wire format. When converting
 * back from wire format, all headers are marked as non-sensitive.
 *
 * @ingroup sdk_internal
 */

namespace SpacetimeDB {
namespace convert {

// ==================== HttpMethod Conversions ====================

/**
 * @brief Convert user-facing HttpMethod to wire format
 *
 * Standard methods (GET, POST, etc.) map to unit enum variants.
 * Non-standard methods are stored in the Extension variant.
 */
inline wire::HttpMethod to_wire(const HttpMethod& method) {
    wire::HttpMethod result;
    
    // Check for standard methods
    if (method.value == "GET") {
        result.tag = wire::HttpMethod::Tag::Get;
    } else if (method.value == "HEAD") {
        result.tag = wire::HttpMethod::Tag::Head;
    } else if (method.value == "POST") {
        result.tag = wire::HttpMethod::Tag::Post;
    } else if (method.value == "PUT") {
        result.tag = wire::HttpMethod::Tag::Put;
    } else if (method.value == "DELETE") {
        result.tag = wire::HttpMethod::Tag::Delete;
    } else if (method.value == "CONNECT") {
        result.tag = wire::HttpMethod::Tag::Connect;
    } else if (method.value == "OPTIONS") {
        result.tag = wire::HttpMethod::Tag::Options;
    } else if (method.value == "TRACE") {
        result.tag = wire::HttpMethod::Tag::Trace;
    } else if (method.value == "PATCH") {
        result.tag = wire::HttpMethod::Tag::Patch;
    } else {
        // Non-standard method - store in Extension variant
        result.tag = wire::HttpMethod::Tag::Extension;
        result.extension = method.value;
    }
    
    return result;
}

/**
 * @brief Convert wire format HttpMethod to user-facing type
 */
inline HttpMethod from_wire(const wire::HttpMethod& method) {
    switch (method.tag) {
        case wire::HttpMethod::Tag::Get:
            return HttpMethod::get();
        case wire::HttpMethod::Tag::Head:
            return HttpMethod::head();
        case wire::HttpMethod::Tag::Post:
            return HttpMethod::post();
        case wire::HttpMethod::Tag::Put:
            return HttpMethod::put();
        case wire::HttpMethod::Tag::Delete:
            return HttpMethod::del();
        case wire::HttpMethod::Tag::Connect:
            return HttpMethod::connect();
        case wire::HttpMethod::Tag::Options:
            return HttpMethod::options();
        case wire::HttpMethod::Tag::Trace:
            return HttpMethod::trace();
        case wire::HttpMethod::Tag::Patch:
            return HttpMethod::patch();
        case wire::HttpMethod::Tag::Extension:
            return HttpMethod{method.extension};
        default:
            // Should never happen, but default to GET for safety
            return HttpMethod::get();
    }
}

// ==================== HttpVersion Conversions ====================

/**
 * @brief Convert user-facing HttpVersion to wire format
 */
inline wire::HttpVersion to_wire(HttpVersion version) {
    wire::HttpVersion result;
    
    switch (version) {
        case HttpVersion::Http09:
            result.tag = wire::HttpVersion::Tag::Http09;
            break;
        case HttpVersion::Http10:
            result.tag = wire::HttpVersion::Tag::Http10;
            break;
        case HttpVersion::Http11:
            result.tag = wire::HttpVersion::Tag::Http11;
            break;
        case HttpVersion::Http2:
            result.tag = wire::HttpVersion::Tag::Http2;
            break;
        case HttpVersion::Http3:
            result.tag = wire::HttpVersion::Tag::Http3;
            break;
    }
    
    return result;
}

/**
 * @brief Convert wire format HttpVersion to user-facing type
 */
inline HttpVersion from_wire(const wire::HttpVersion& version) {
    switch (version.tag) {
        case wire::HttpVersion::Tag::Http09:
            return HttpVersion::Http09;
        case wire::HttpVersion::Tag::Http10:
            return HttpVersion::Http10;
        case wire::HttpVersion::Tag::Http11:
            return HttpVersion::Http11;
        case wire::HttpVersion::Tag::Http2:
            return HttpVersion::Http2;
        case wire::HttpVersion::Tag::Http3:
            return HttpVersion::Http3;
        default:
            // Should never happen, default to HTTP/1.1
            return HttpVersion::Http11;
    }
}

// ==================== HttpHeader Conversions ====================

/**
 * @brief Convert user-facing HttpHeader to wire format HttpHeaderPair
 *
 * WARNING: The is_sensitive flag is LOST during this conversion.
 * The wire format does not preserve sensitivity information.
 */
inline wire::HttpHeaderPair to_wire(const HttpHeader& header) {
    wire::HttpHeaderPair result;
    result.name = header.name;
    result.value = header.value;
    return result;
}

/**
 * @brief Convert wire format HttpHeaderPair to user-facing HttpHeader
 *
 * The resulting header will have is_sensitive=false, as the wire format
 * does not preserve sensitivity information.
 */
inline HttpHeader from_wire(const wire::HttpHeaderPair& pair) {
    return HttpHeader{pair.name, pair.value, false};
}

// ==================== HttpHeaders (collection) Conversions ====================

/**
 * @brief Convert user-facing header vector to wire format HttpHeaders
 */
inline wire::HttpHeaders to_wire_headers(const std::vector<HttpHeader>& headers) {
    wire::HttpHeaders result;
    result.entries.reserve(headers.size());
    
    for (const auto& header : headers) {
        result.entries.push_back(to_wire(header));
    }
    
    return result;
}

/**
 * @brief Convert wire format HttpHeaders to user-facing header vector
 */
inline std::vector<HttpHeader> from_wire_headers(const wire::HttpHeaders& headers) {
    std::vector<HttpHeader> result;
    result.reserve(headers.entries.size());
    
    for (const auto& pair : headers.entries) {
        result.push_back(from_wire(pair));
    }
    
    return result;
}

// ==================== HttpRequest Conversions ====================

/**
 * @brief Convert user-facing HttpRequest to wire format
 *
 * Note: The body field is NOT included in the wire HttpRequest struct.
 * The body bytes are passed separately via ConsumeBytes().
 */
inline wire::HttpRequest to_wire(const HttpRequest& request) {
    wire::HttpRequest result;
    result.method = to_wire(request.method);
    result.headers = to_wire_headers(request.headers);
    result.timeout = request.timeout;
    result.uri = request.uri;
    result.version = to_wire(request.version);
    return result;
}

/**
 * @brief Convert wire format HttpRequest to user-facing type
 *
 * Note: The returned HttpRequest will have an empty body.
 * The body bytes are received separately via ConsumeBytes() and must be set manually.
 */
inline HttpRequest from_wire(const wire::HttpRequest& request) {
    HttpRequest result;
    result.method = from_wire(request.method);
    result.headers = from_wire_headers(request.headers);
    result.timeout = request.timeout;
    result.uri = request.uri;
    result.version = from_wire(request.version);
    result.body = HttpBody::empty(); // Body is received separately
    return result;
}

// ==================== HttpResponse Conversions ====================

/**
 * @brief Convert user-facing HttpResponse to wire format
 *
 * Note: The body field is NOT included in the wire HttpResponse struct.
 * The body bytes are passed separately via ConsumeBytes().
 */
inline wire::HttpResponse to_wire(const HttpResponse& response) {
    wire::HttpResponse result;
    result.headers = to_wire_headers(response.headers);
    result.version = to_wire(response.version);
    result.code = response.status_code;
    return result;
}

/**
 * @brief Convert wire format HttpResponse to user-facing type
 *
 * Note: The returned HttpResponse will have an empty body.
 * The body bytes are received separately via ConsumeBytes() and must be set manually.
 */
inline HttpResponse from_wire(const wire::HttpResponse& response) {
    HttpResponse result;
    result.headers = from_wire_headers(response.headers);
    result.version = from_wire(response.version);
    result.status_code = response.code;
    result.body = HttpBody::empty(); // Body is received separately
    return result;
}

} // namespace convert
} // namespace SpacetimeDB

#endif // SPACETIMEDB_HTTP_CONVERT_H
