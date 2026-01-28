#ifndef SPACETIMEDB_HTTP_H
#define SPACETIMEDB_HTTP_H

#pragma once

#include <string>
#include <vector>
#include <optional>
#include <cstdint>
#include "spacetimedb/bsatn/time_duration.h"
#include "spacetimedb/outcome.h"

/**
 * @file http.h
 * @brief HTTP request support for SpacetimeDB procedures
 *
 * This module provides types and functionality for making outbound HTTP requests
 * from within SpacetimeDB procedures. The actual network I/O is performed by the
 * SpacetimeDB host using reqwest (Rust), ensuring security and resource management.
 *
 * IMPORTANT LIMITATIONS:
 * - HTTP requests CANNOT be performed inside with_tx() or try_with_tx()
 * - The host will reject HTTP requests with WOULD_BLOCK_TRANSACTION
 * - All timeouts are clamped to a maximum of 500ms by the host
 * - No external HTTP library dependencies (host does actual HTTP)
 *
 * Example usage:
 * @code
 * SPACETIMEDB_PROCEDURE(std::string, fetch_data, ProcedureContext ctx) {
 *     auto result = ctx.http.get("http://api.example.com/data");
 *     
 *     if (result.is_ok()) {
 *         auto& response = result.value();
 *         return Ok(response.body.to_string_utf8_lossy());
 *     } else {
 *         return Err("HTTP error: " + result.error());
 *     }
 * }
 * @endcode
 *
 * @ingroup sdk_runtime
 */

namespace SpacetimeDB {

/**
 * @brief HTTP method (e.g., GET, POST, PUT, DELETE)
 *
 * Supports all standard HTTP methods plus custom extension methods.
 * Extension methods are any string value not matching a standard method.
 */
struct HttpMethod {
    std::string value;
    
    /// Standard HTTP methods
    static HttpMethod get() { return HttpMethod{"GET"}; }
    static HttpMethod head() { return HttpMethod{"HEAD"}; }
    static HttpMethod post() { return HttpMethod{"POST"}; }
    static HttpMethod put() { return HttpMethod{"PUT"}; }
    // DELETE cannot be named "delete" in C++; provide snake_case aliases
    static HttpMethod del() { return HttpMethod{"DELETE"}; }
    static HttpMethod http_delete() { return HttpMethod{"DELETE"}; }
    static HttpMethod connect() { return HttpMethod{"CONNECT"}; }
    static HttpMethod options() { return HttpMethod{"OPTIONS"}; }
    static HttpMethod trace() { return HttpMethod{"TRACE"}; }
    static HttpMethod patch() { return HttpMethod{"PATCH"}; }
    
    /// Create a custom/extension HTTP method
    explicit HttpMethod(std::string v) : value(std::move(v)) {}
};

/**
 * @brief HTTP protocol version
 */
enum class HttpVersion : uint8_t {
    Http09,  ///< HTTP/0.9
    Http10,  ///< HTTP/1.0
    Http11,  ///< HTTP/1.1 (default)
    Http2,   ///< HTTP/2
    Http3,   ///< HTTP/3
};

/**
 * @brief HTTP header name/value pair
 *
 * Header values are always treated as raw bytes. The is_sensitive flag
 * is a local-only hint and is not transmitted to the host (sensitive
 * headers have their names redacted in wire format).
 */
struct HttpHeader {
    std::string name;
    std::vector<uint8_t> value;
    bool is_sensitive = false;
    
    /**
     * @brief Create header from string name and string value
     * @param n Header name
     * @param v Header value (converted to ASCII bytes)
     * @param sensitive If true, header name will be redacted in logs/wire format
     */
    HttpHeader(std::string n, std::string v, bool sensitive = false)
        : name(std::move(n))
        , value(v.begin(), v.end())
        , is_sensitive(sensitive) 
    {}
    
    /**
     * @brief Create header from string name and byte value
     * @param n Header name
     * @param v Header value as raw bytes
     * @param sensitive If true, header name will be redacted in logs/wire format
     */
    HttpHeader(std::string n, std::vector<uint8_t> v, bool sensitive = false)
        : name(std::move(n))
        , value(std::move(v))
        , is_sensitive(sensitive) 
    {}
};

/**
 * @brief HTTP request/response body
 *
 * Bodies are always treated as raw bytes. Use helper methods for UTF-8 text.
 */
struct HttpBody {
    std::vector<uint8_t> bytes;
    
    /// Create an empty body
    static HttpBody empty() { 
        return HttpBody{std::vector<uint8_t>()}; 
    }
    
    /// Create body from UTF-8 string
    static HttpBody from_string(const std::string& s) {
        return HttpBody{std::vector<uint8_t>(s.begin(), s.end())};
    }
    
    /// Get body bytes
    std::vector<uint8_t> to_bytes() const {
        return bytes;
    }
    
    /// Convert body to UTF-8 string (lossy conversion)
    std::string to_string_utf8_lossy() const {
        return std::string(bytes.begin(), bytes.end());
    }
    
    /// Check if body is empty
    bool is_empty() const {
        return bytes.empty();
    }
};

/**
 * @brief HTTP request to be executed by the SpacetimeDB host
 *
 * Use designated initializers (C++20) to construct requests:
 * @code
 * HttpRequest request{
 *     .uri = "http://example.com/api",
 *     .method = HttpMethod::post(),
 *     .headers = {HttpHeader{"Content-Type", "application/json"}},
 *     .body = HttpBody::from_string("{\"key\": \"value\"}"),
 *     .timeout = TimeDuration::from_millis(100)
 * };
 * @endcode
 *
 * The host clamps all timeouts to a maximum of 500ms.
 */
struct HttpRequest {
    std::string uri;
    HttpMethod method = HttpMethod::get();
    std::vector<HttpHeader> headers;
    HttpBody body = HttpBody::empty();
    HttpVersion version = HttpVersion::Http11;
    std::optional<TimeDuration> timeout;
};

/**
 * @brief HTTP response returned by the SpacetimeDB host
 *
 * A non-2xx status code is still returned as a successful response; callers should
 * inspect status_code to handle application-level errors from the remote server.
 */
struct HttpResponse {
    uint16_t status_code;
    HttpVersion version;
    std::vector<HttpHeader> headers;
    HttpBody body;
};

/**
 * @brief HTTP client for making outbound HTTP requests via the host
 *
 * Available from ProcedureContext.http
 *
 * Returns Outcome<HttpResponse> where:
 * - Ok(response): Request succeeded (including non-2xx status codes)
 * - Err(message): Transport error (DNS, connection, timeout)
 *
 * IMPORTANT: Do NOT call inside WithTx() or TryWithTx()
 * The host will reject HTTP requests while a transaction is open
 * and return WOULD_BLOCK_TRANSACTION error.
 */
class HttpClient {
public:
    /**
     * @brief Send a simple GET request
     *
     * @param uri The request URI
     * @param timeout Optional timeout (clamped to 500ms by host)
     * @return Outcome<HttpResponse> - Ok if response received, Err if transport failed
     *
     * @code
     * auto result = ctx.http.get("http://localhost:3000/v1/database/schema");
     * if (result.is_ok()) {
     *     auto& response = result.value();
     *     return Ok(response.body.to_string_utf8_lossy());
     * } else {
     *     return Err("HTTP error: " + result.error());
     * }
     * @endcode
     */
    Outcome<HttpResponse> get(
        const std::string& uri, 
        std::optional<TimeDuration> timeout = std::nullopt
    ) {
        HttpRequest request{
            .uri = uri,
            .method = HttpMethod::get(),
            .headers = {},
            .body = HttpBody::empty(),
            .version = HttpVersion::Http11,
            .timeout = timeout
        };
        return send(request);
    }
    
    /**
     * @brief Send an HTTP request
     *
     * @param request The HTTP request to send
     * @return Outcome<HttpResponse> - Ok if response received, Err if transport failed
     *
     * This method does not throw for expected failures; errors are returned as Outcome::Err.
     *
     * Example with POST:
     * @code
     * auto request = HttpRequest{
     *     .uri = "https://api.example.com/upload",
     *     .method = HttpMethod::post(),
     *     .headers = {HttpHeader{"Content-Type", "text/plain"}},
     *     .body = HttpBody::from_string("This is the request body"),
     *     .timeout = TimeDuration::from_millis(100)
     * };
     *
     * auto result = ctx.http.send(request);
     * if (result.is_ok()) {
     *     auto& response = result.value();
     *     return Ok("Status: " + std::to_string(response.status_code));
     * } else {
     *     return Err("Error: " + result.error());
     * }
     * @endcode
     *
     * Example handling 404:
     * @code
     * auto result = ctx.http.get("https://example.com/missing");
     * if (!result.is_ok()) {
     *     // Transport error (DNS failure, connection drop, timeout, etc.)
     *     return Err("Transport error: " + result.error());
     * }
     *
     * auto& response = result.value();
     * if (response.status_code != 200) {
     *     // Application-level HTTP error response
     *     return Err("HTTP status: " + std::to_string(response.status_code));
     * }
     *
     * return Ok(response.body.to_string_utf8_lossy());
     * @endcode
     *
     * Example showing transaction blocking:
     * @code
     * // ✗ WRONG: This will fail with WOULD_BLOCK_TRANSACTION
     * ctx.WithTx([](TxContext& tx) {
     *     auto result = ctx.http.get("https://example.com/");
     *     // ERROR: HTTP blocked in transaction
     * });
     *
     * // ✓ CORRECT: HTTP before transaction
     * auto api_result = ctx.http.get("https://example.com/");
     * if (api_result.is_ok()) {
     *     ctx.WithTx([&api_result](TxContext& tx) {
     *         // Use api_result data here
     *     });
     * }
     * @endcode
     */
    Outcome<HttpResponse> send(const HttpRequest& request) {
        #ifndef SPACETIMEDB_UNSTABLE_FEATURES
        return Err<HttpResponse>("HTTP requests require SPACETIMEDB_UNSTABLE_FEATURES to be enabled");
        #else
        // Implemented in http_client_impl.h to avoid circular dependencies
        return SendImpl(request);
        #endif
    }

private:
    Outcome<HttpResponse> SendImpl(const HttpRequest& request);
};

} // namespace SpacetimeDB

// Include implementation after class definition to avoid circular dependencies
#ifdef SPACETIMEDB_UNSTABLE_FEATURES
#include "spacetimedb/http_client_impl.h"
#endif

#endif // SPACETIMEDB_HTTP_H
