#include "spacetimedb/jwt_claims.h"
#include <stdexcept>
#include <cstring>

namespace SpacetimeDb {

// Simple JSON string extraction helper
// Finds "key":"value" and returns the value
std::optional<std::string> JwtClaims::ExtractStringClaim(const std::string& json, const std::string& key) {
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
std::vector<std::string> JwtClaims::ExtractAudienceClaim(const std::string& json) {
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

JwtClaims::JwtClaims(std::string jwt_payload, Identity identity)
    : payload_(std::move(jwt_payload)), identity_(std::move(identity)) {}

const std::string& JwtClaims::Subject() const {
    if (!subject_.has_value()) {
        subject_ = ExtractStringClaim(payload_, "sub");
        if (!subject_.has_value()) {
            // Return empty string on error instead of throwing
            static const std::string empty;
            return empty;
        }
    }
    return *subject_;
}

const std::string& JwtClaims::Issuer() const {
    if (!issuer_.has_value()) {
        issuer_ = ExtractStringClaim(payload_, "iss");
        if (!issuer_.has_value()) {
            // Return empty string on error instead of throwing
            static const std::string empty;
            return empty;
        }
    }
    return *issuer_;
}

const std::vector<std::string>& JwtClaims::Audience() const {
    if (!audience_.has_value()) {
        audience_ = ExtractAudienceClaim(payload_);
    }
    return *audience_;
}

} // namespace SpacetimeDb
