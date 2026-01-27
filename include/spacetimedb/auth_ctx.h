#ifndef SPACETIMEDB_AUTH_CTX_H
#define SPACETIMEDB_AUTH_CTX_H

#include "spacetimedb/jwt_claims.h"
#include "spacetimedb/bsatn/types.h"
#include "spacetimedb/abi/FFI.h"
#include "spacetimedb/abi/opaque_types.h"
#include <memory>
#include <optional>
#include <functional>
#include <vector>
#include <array>

namespace SpacetimeDB {

// Forward declarations
struct ConnectionId;

/**
 * @brief Authentication context for a reducer call.
 * 
 * Provides access to the JWT claims for the connection that triggered the reducer,
 * if any. Reducers can be called from internal sources (scheduled reducers, init, etc.)
 * or from external connections (with potential JWT authentication).
 * 
 * This class uses lazy loading - the JWT is only fetched and parsed when accessed.
 */
class AuthCtx {
private:
    bool is_internal_;
    mutable std::shared_ptr<std::optional<JwtClaims>> jwt_;
    std::function<std::optional<JwtClaims>()> jwt_loader_;

    // Private constructor used by factory methods
    AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader);

public:
    /**
     * @brief Creates an AuthCtx from an optional ConnectionId.
     * 
     * If the connection_id is present, creates an AuthCtx that will load the JWT.
     * If the connection_id is absent, creates an internal AuthCtx.
     * 
     * @param connection_id Optional connection ID
     * @param sender The identity of the caller (already derived from JWT claims by the host)
     * @return An AuthCtx based on the connection_id
     */
    static AuthCtx from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender);

    /**
     * @brief Creates an AuthCtx for an internal (non-connection-based) reducer call.
     * 
     * Internal calls include scheduled reducers, init reducers, and other
     * database-initiated operations.
     * 
     * @return An AuthCtx representing an internal call
     */
    static AuthCtx internal();

    /**
     * @brief Creates an AuthCtx from a JWT payload string.
     * 
     * This is primarily used for testing purposes, allowing you to create
     * an AuthCtx with specific JWT claims without needing a real connection.
     * 
     * Note: The Identity must be computed by calling the host function,
     * as we cannot compute Blake3 hashes in WASM.
     * 
     * @param jwt_payload The raw JWT payload (JSON claims)
     * @param identity The identity derived from the JWT's issuer and subject
     * @return An AuthCtx with the provided JWT
     */
    static AuthCtx from_jwt_payload(std::string jwt_payload, Identity identity);

    /**
     * @brief Creates an AuthCtx that reads the JWT for the given connection ID.
     * 
     * The JWT will be lazily loaded from the host when first accessed.
     * The identity parameter is the sender's identity, already derived from
     * JWT claims by the host (using Blake3 hashing).
     * 
     * @param connection_id The connection ID to load the JWT for
     * @param sender The identity of the caller (already derived from JWT claims by the host)
     * @return An AuthCtx that will load the JWT on demand
     */
    static AuthCtx from_connection_id(ConnectionId connection_id, Identity sender);

    /**
     * @brief Returns whether this reducer was spawned from inside the database.
     * 
     * @return true if this is an internal call (scheduled, init, etc.)
     */
    bool is_internal() const { return is_internal_; }

    /**
     * @brief Checks if there is a JWT without loading it.
     * 
     * If is_internal() returns true, this will return false.
     * 
     * @return true if a JWT is available
     */
    bool has_jwt() const;

    /**
     * @brief Gets the JWT claims, loading them if necessary.
     * 
     * This will fetch the JWT from the host on the first call and cache it.
     * 
     * @return An optional containing the JwtClaims if available
     */
    const std::optional<JwtClaims>& get_jwt() const;

    /**
     * @brief Gets the caller's identity.
     * 
     * For internal calls, this returns the database's identity.
     * For external calls, this returns the identity derived from the JWT
     * (based on the issuer and subject claims).
     * 
     * @return The caller's Identity
     */
    Identity get_caller_identity() const;
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

constexpr uint16_t ERROR_BUFFER_TOO_SMALL = 11;

inline AuthCtx::AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader)
    : is_internal_(is_internal), jwt_loader_(std::move(loader)) {}

inline AuthCtx AuthCtx::from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender) {
    if (connection_id.has_value()) {
        return from_connection_id(*connection_id, std::move(sender));
    } else {
        return internal();
    }
}

inline AuthCtx AuthCtx::internal() {
    return AuthCtx(true, []() -> std::optional<JwtClaims> { return std::nullopt; });
}

inline AuthCtx AuthCtx::from_jwt_payload(std::string jwt_payload, Identity identity) {
    return AuthCtx(false, [payload = std::move(jwt_payload), id = std::move(identity)]() mutable -> std::optional<JwtClaims> {
        return JwtClaims(std::move(payload), std::move(id));
    });
}

inline AuthCtx AuthCtx::from_connection_id(ConnectionId connection_id, Identity sender) {
    return AuthCtx(false, [connection_id, sender]() -> std::optional<JwtClaims> {
        // Call the host FFI to get the JWT
        BytesSource jwt_source;
        
        // Convert ConnectionId to byte array (little-endian)
        std::array<uint8_t, 16> conn_id_bytes;
        for (int i = 0; i < 8; ++i) {
            conn_id_bytes[i] = (connection_id.id.low >> (i * 8)) & 0xFF;
        }
        for (int i = 0; i < 8; ++i) {
            conn_id_bytes[8 + i] = (connection_id.id.high >> (i * 8)) & 0xFF;
        }
        
        Status status = FFI::get_jwt(conn_id_bytes.data(), &jwt_source);
        if (status != Status(0) || jwt_source == BytesSource{0}) {
            return std::nullopt;
        }
        
        // Read the JWT payload from the BytesSource
        std::vector<uint8_t> buffer;
        buffer.resize(4096); // Start with 4KB buffer
        
        size_t buffer_len = buffer.size();
        int16_t result = bytes_source_read(jwt_source, buffer.data(), &buffer_len);
        
        while (result == ERROR_BUFFER_TOO_SMALL) {
            buffer.resize(buffer.size() * 2);
            buffer_len = buffer.size();
            result = bytes_source_read(jwt_source, buffer.data(), &buffer_len);
        }
        
        if (result < 0) {
            return std::nullopt;
        }
        
        // Convert bytes to string
        std::string jwt_payload(buffer.begin(), buffer.begin() + buffer_len);
        
        // Use the provided sender identity (already computed by host from JWT claims)
        return JwtClaims(std::move(jwt_payload), sender);
    });
}

inline bool AuthCtx::has_jwt() const {
    if (is_internal_) {
        return false;
    }
    
    // Load the JWT if not already loaded, then check if it has a value
    // This ensures has_jwt() and get_jwt() are consistent
    return get_jwt().has_value();
}

inline const std::optional<JwtClaims>& AuthCtx::get_jwt() const {
    if (!jwt_) {
        jwt_ = std::make_shared<std::optional<JwtClaims>>(jwt_loader_());
    }
    return *jwt_;
}

inline Identity AuthCtx::get_caller_identity() const {
    if (is_internal_) {
        // Return database identity for internal calls
        std::array<uint8_t, 32> identity_bytes;
        FFI::identity(identity_bytes.data());
        return Identity(identity_bytes);
    }
    
    const auto& jwt = get_jwt();
    if (jwt.has_value()) {
        return jwt->get_identity();
    }
    
    // No JWT, return database identity as fallback
    std::array<uint8_t, 32> identity_bytes;
    FFI::identity(identity_bytes.data());
    return Identity(identity_bytes);
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_AUTH_CTX_H
