#ifndef SPACETIMEDB_JWT_CLAIMS_H
#define SPACETIMEDB_JWT_CLAIMS_H

#include "spacetimedb/bsatn/types.h"
#include <string>
#include <vector>
#include <memory>
#include <optional>

namespace SpacetimeDB {

/**
 * @brief Represents the claims from a JSON Web Token (JWT).
 * 
 * This class provides lazy parsing of JWT claims, parsing specific fields
 * on demand. It follows the same pattern as the Rust and C# implementations.
 * 
 * The Identity is provided in the constructor because computing it requires
 * Blake3 hashing, which is done on the host side.
 */
class JwtClaims {
private:
    std::string payload_;
    Identity identity_;
    mutable std::optional<std::string> subject_;
    mutable std::optional<std::string> issuer_;
    mutable std::optional<std::vector<std::string>> audience_;

    // Helper to extract a string claim from JSON
    static std::optional<std::string> extract_string_claim(const std::string& json, const std::string& key);
    
    // Helper to extract the audience claim (can be string or array)
    static std::vector<std::string> extract_audience_claim(const std::string& json);

public:
    /**
     * @brief Constructs a JwtClaims from a JWT payload and its associated Identity.
     * 
     * The Identity must be provided because computing it requires Blake3 hashing,
     * which is performed on the host side.
     * 
     * @param jwt_payload The raw JWT payload (JSON claims)
     * @param identity The identity derived from the JWT's issuer and subject
     */
    JwtClaims(std::string jwt_payload, Identity identity);

    /**
     * @brief Returns the token's subject from the 'sub' claim.
     * 
     * @return The subject string, or empty string if missing/invalid
     */
    const std::string& subject() const;

    /**
     * @brief Returns the issuer for these credentials from the 'iss' claim.
     * 
     * @return The issuer string, or empty string if missing/invalid
     */
    const std::string& issuer() const;

    /**
     * @brief Returns the audience for these credentials from the 'aud' claim.
     * 
     * The audience can be either a single string or an array of strings.
     * This method returns a vector that will contain either 0, 1, or multiple strings.
     * 
     * @return A vector of audience strings
     */
    const std::vector<std::string>& audience() const;

    /**
     * @brief Returns the identity for these credentials.
     * 
     * The identity is based on the 'iss' and 'sub' claims and is computed
     * using Blake3 hashing on the host side.
     * 
     * @return The identity
     */
    const Identity& get_identity() const { return identity_; }

    /**
     * @brief Returns the whole JWT payload as a JSON string.
     * 
     * @return The raw JWT payload
     */
    const std::string& raw_payload() const { return payload_; }
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

// Simple JSON string extraction helper
// Finds "key":"value" and returns the value
inline std::optional<std::string> JwtClaims::extract_string_claim(const std::string& json, const std::string& key) {
    // Look for "key":
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) {
        return std::nullopt;
    }
    
    pos += search.length();
    
    // Skip whitespace
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n')) {
        pos++;
    }
    
    // Check if the value is a string (starts with ")
    if (pos >= json.length() || json[pos] != '"') {
        return std::nullopt;
    }
    
    pos++; // Skip opening quote
    size_t end_pos = pos;
    
    // Find closing quote, handling escaped quotes
    while (end_pos < json.length()) {
        if (json[end_pos] == '"' && (end_pos == pos || json[end_pos - 1] != '\\')) {
            break;
        }
        end_pos++;
    }
    
    if (end_pos >= json.length()) {
        return std::nullopt;
    }
    
    return json.substr(pos, end_pos - pos);
}

// Extract audience claim - can be string or array
inline std::vector<std::string> JwtClaims::extract_audience_claim(const std::string& json) {
    std::vector<std::string> result;
    
    // Look for "aud":
    std::string search = "\"aud\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) {
        return result; // No audience claim
    }
    
    pos += search.length();
    
    // Skip whitespace
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n')) {
        pos++;
    }
    
    if (pos >= json.length()) {
        return result;
    }
    
    // Check if it's an array or a single string
    if (json[pos] == '[') {
        // Array case
        pos++; // Skip [
        
        while (pos < json.length()) {
            // Skip whitespace
            while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == ',')) {
                pos++;
            }
            
            if (pos >= json.length() || json[pos] == ']') {
                break;
            }
            
            if (json[pos] == '"') {
                pos++; // Skip opening quote
                size_t end_pos = pos;
                
                // Find closing quote
                while (end_pos < json.length() && json[end_pos] != '"') {
                    if (json[end_pos] == '\\') {
                        end_pos++; // Skip escaped character
                    }
                    end_pos++;
                }
                
                if (end_pos < json.length()) {
                    result.push_back(json.substr(pos, end_pos - pos));
                    pos = end_pos + 1;
                }
            } else {
                break; // Invalid format
            }
        }
    } else if (json[pos] == '"') {
        // Single string case
        pos++; // Skip opening quote
        size_t end_pos = pos;
        
        while (end_pos < json.length() && json[end_pos] != '"') {
            if (json[end_pos] == '\\') {
                end_pos++; // Skip escaped character
            }
            end_pos++;
        }
        
        if (end_pos < json.length()) {
            result.push_back(json.substr(pos, end_pos - pos));
        }
    }
    
    return result;
}

inline JwtClaims::JwtClaims(std::string jwt_payload, Identity identity)
    : payload_(std::move(jwt_payload)), identity_(std::move(identity)) {}

inline const std::string& JwtClaims::subject() const {
    if (!subject_.has_value()) {
        subject_ = extract_string_claim(payload_, "sub");
        if (!subject_.has_value()) {
            // Return empty string on error instead of throwing
            static const std::string empty;
            return empty;
        }
    }
    return *subject_;
}

inline const std::string& JwtClaims::issuer() const {
    if (!issuer_.has_value()) {
        issuer_ = extract_string_claim(payload_, "iss");
        if (!issuer_.has_value()) {
            // Return empty string on error instead of throwing
            static const std::string empty;
            return empty;
        }
    }
    return *issuer_;
}

inline const std::vector<std::string>& JwtClaims::audience() const {
    if (!audience_.has_value()) {
        audience_ = extract_audience_claim(payload_);
    }
    return *audience_;
}

} // namespace SpacetimeDB

#endif // SPACETIMEDB_JWT_CLAIMS_H
