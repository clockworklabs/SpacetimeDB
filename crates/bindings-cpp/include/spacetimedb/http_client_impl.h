#ifndef SPACETIMEDB_HTTP_CLIENT_IMPL_H
#define SPACETIMEDB_HTTP_CLIENT_IMPL_H

#pragma once

#include "spacetimedb/http_wire.h"
#include "spacetimedb/http_convert.h"
#include "spacetimedb/abi/abi.h"
#include "spacetimedb/bsatn/bsatn.h"
#include "spacetimedb/internal/Module.h"

namespace SpacetimeDB {

inline Outcome<HttpResponse> HttpClient::SendImpl(const HttpRequest& request) {
    // Convert user-facing request to wire format
    wire::HttpRequest wire_request = convert::to_wire(request);
    
    // Serialize wire request to BSATN
    bsatn::Writer writer;
    bsatn::serialize(writer, wire_request);
    std::vector<uint8_t> request_bytes = writer.take_buffer();
    
    // Prepare body bytes
    const std::vector<uint8_t>& body_bytes = request.body.bytes;
    
    // Call host function
    // Note: For empty body, we need to pass a valid pointer, not null
    const uint8_t* body_ptr = body_bytes.empty() ? reinterpret_cast<const uint8_t*>("") : body_bytes.data();
    
    BytesSource out[2] = {BytesSource{0}, BytesSource{0}};
    Status status = procedure_http_request(
        request_bytes.data(), request_bytes.size(),
        body_ptr, body_bytes.size(),
        out
    );
    
    // Check for errors
    if (status.inner != 0) {
        // HTTP_ERROR (21) means the HTTP call failed - error message is in out[0]
        if (status.inner == 21) {
            // Read error message from out[0]
            std::vector<uint8_t> error_bytes = Internal::ConsumeBytes(out[0]);
            
            LOG_INFO("HTTP: Error bytes: " + std::to_string(error_bytes.size()));
            
            // Decode BSATN string
            bsatn::Reader reader(error_bytes.data(), error_bytes.size());
            std::string error_message = bsatn::deserialize<std::string>(reader);
            
            LOG_INFO("HTTP: Error message: " + error_message);
            
            return Err<HttpResponse>(std::move(error_message));
        }
        
        // Other errors (WOULD_BLOCK_TRANSACTION, etc.)
        if (status.inner == 17) {
            return Err<HttpResponse>("HTTP requests are blocked inside transactions. Call HTTP before with_tx() or try_with_tx().");
        }
        
        LOG_INFO("HTTP: Unknown error code: " + std::to_string(status.inner));
        return Err<HttpResponse>("HTTP request failed with status code: " + std::to_string(status.inner));
    }
    
    // Success - decode response from out[0] and body from out[1]
    std::vector<uint8_t> response_bytes = Internal::ConsumeBytes(out[0]);
    std::vector<uint8_t> response_body_bytes = Internal::ConsumeBytes(out[1]);
    
    // Decode wire response
    bsatn::Reader response_reader(response_bytes.data(), response_bytes.size());
    wire::HttpResponse wire_response = bsatn::deserialize<wire::HttpResponse>(response_reader);
    
    // Convert wire response to user-facing type
    HttpResponse response = convert::from_wire(wire_response);
    
    // Set the body
    response.body = HttpBody{std::move(response_body_bytes)};
    
    return Ok(std::move(response));
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_HTTP_CLIENT_IMPL_H
