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
    std::optional<Identity> verified_sender_;
    mutable std::shared_ptr<std::optional<JwtClaims>> jwt_;
    std::function<std::optional<JwtClaims>()> jwt_loader_;

    // Private constructor used by factory methods
    AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader,
            std::optional<Identity> verified_sender = std::nullopt);
    static AuthCtx from_connection_with_flags(ConnectionId connection_id, Identity sender, uint32_t flags);

public:
    /**
     * @brief Creates an AuthCtx from an optional ConnectionId.
     * 
     * If the connection_id is present, creates an AuthCtx that will load the JWT.
     * Internal authority is captured from the host, independently of connection presence.
     * 
     * @param connection_id Optional connection ID
     * @param sender The verified caller Identity supplied by the host
     * @return An AuthCtx with captured invocation authority and lazy JWT loading
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
     * The Identity must be the verified sender supplied by the host.
     * 
     * @param jwt_payload The raw JWT payload (JSON claims)
     * @param identity The verified sender Identity
     * @return An AuthCtx with the provided JWT
     */
    static AuthCtx from_jwt_payload(std::string jwt_payload, Identity identity);

    /**
     * @brief Creates an AuthCtx that reads the JWT for the given connection ID.
     * 
     * The JWT will be lazily loaded from the host when first accessed.
     * The identity parameter is the verified sender supplied by the host.
     * 
     * @param connection_id The connection ID to load the JWT for
     * @param sender The verified sender Identity supplied by the host
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
     * @brief Checks if there is a JWT, loading it lazily if necessary.
     * 
     * Independent of is_internal(). Internal calls can also have a JWT.
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
     * Returns the verified sender captured when constructing the context,
     * independently of JWT presence or token claims.
     * 
     * @return The caller's Identity
     */
    Identity get_caller_identity() const;
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

inline AuthCtx::AuthCtx(bool is_internal, std::function<std::optional<JwtClaims>()> loader,
                       std::optional<Identity> verified_sender)
    : is_internal_(is_internal), verified_sender_(std::move(verified_sender)), jwt_loader_(std::move(loader)) {}

inline AuthCtx AuthCtx::from_connection_id_opt(std::optional<ConnectionId> connection_id, Identity sender) {
    const auto flags = FFI::get_call_auth_flags();
    if (connection_id.has_value()) {
        return from_connection_with_flags(*connection_id, std::move(sender), flags);
    } else {
        return AuthCtx((flags & 1) != 0, []() -> std::optional<JwtClaims> { return std::nullopt; }, sender);
    }
}

inline AuthCtx AuthCtx::internal() {
    return AuthCtx(true, []() -> std::optional<JwtClaims> { return std::nullopt; });
}

inline AuthCtx AuthCtx::from_jwt_payload(std::string jwt_payload, Identity identity) {
    return AuthCtx(false, [payload = std::move(jwt_payload), id = identity]() mutable -> std::optional<JwtClaims> {
        return JwtClaims(std::move(payload), std::move(id));
    }, identity);
}

inline AuthCtx AuthCtx::from_connection_id(ConnectionId connection_id, Identity sender) {
    return from_connection_with_flags(connection_id, std::move(sender), FFI::get_call_auth_flags());
}

inline AuthCtx AuthCtx::from_connection_with_flags(ConnectionId connection_id, Identity sender, uint32_t flags) {
    return AuthCtx((flags & 1) != 0, [connection_id, sender]() -> std::optional<JwtClaims> {
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
        std::array<uint8_t, 4096> buffer;
        std::string jwt_payload;
        for (;;) {
            size_t buffer_len = buffer.size();
            const auto result = bytes_source_read(jwt_source, buffer.data(), &buffer_len);
            if (result != 0 && result != -1) return std::nullopt;
            jwt_payload.append(reinterpret_cast<const char*>(buffer.data()), buffer_len);
            // -1 is successful exhaustion and may include the final payload bytes.
            if (result == -1) break;
            if (buffer_len == 0) return std::nullopt;
        }
        if (jwt_payload.empty()) return std::nullopt;
        // Token claims cannot override the verified sender, including hosted tokens.
        return JwtClaims(std::move(jwt_payload), sender);
    }, sender);
}

inline bool AuthCtx::has_jwt() const {
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
    if (verified_sender_.has_value()) return *verified_sender_;
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
