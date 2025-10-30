#pragma once

#include <string>
#include <vector>
#include <optional>
#include <algorithm>
#include <cctype>
#include "bsatn/writer.h"

namespace SpacetimeDb {

// RLS operation types
enum class RlsOperation : uint8_t {
    Select = 0,
    Insert = 1,
    Update = 2,
    Delete = 3
};

// RLS policy definition
struct RlsPolicy {
    std::string table_name;
    std::string policy_name;
    RlsOperation operation;
    std::string sql_condition;
    
    RlsPolicy(const std::string& table, const std::string& policy, 
              RlsOperation op, const std::string& condition)
        : table_name(table), policy_name(policy), operation(op), sql_condition(condition) {}
};

// RLS policy registry
class RlsPolicyRegistry {
private:
    std::vector<RlsPolicy> policies_;
    
    RlsPolicyRegistry() = default;
    
public:
    static RlsPolicyRegistry& instance() {
        static RlsPolicyRegistry registry;
        return registry;
    }
    
    void register_policy(const std::string& table_name, 
                        const std::string& policy_name,
                        RlsOperation operation,
                        const std::string& sql_condition) {
        policies_.emplace_back(table_name, policy_name, operation, sql_condition);
    }
    
    const std::vector<RlsPolicy>& get_policies() const {
        return policies_;
    }
    
    // Write RLS policies to BSATN for module definition
    void write_policies(bsatn::Writer& writer) const {
        writer.write_vec_len(policies_.size());
        
        for (const auto& policy : policies_) {
            // RawRowLevelSecurityDefV9 structure:
            // table_name: String
            writer.write_string(policy.table_name);
            
            // policy_name: String
            writer.write_string(policy.policy_name);
            
            // operations: Vec<RlsOp>
            writer.write_vec_len(1); // Single operation per policy
            writer.write_u8(static_cast<uint8_t>(policy.operation));
            
            // sql: String
            writer.write_string(policy.sql_condition);
        }
    }
};

// Helper function to extract table name from SQL query
inline std::string extract_table_name_from_sql(const std::string& sql) {
    // Simple parser to extract table name from "SELECT * FROM table_name" or similar
    std::string lower_sql = sql;
    std::transform(lower_sql.begin(), lower_sql.end(), lower_sql.begin(), ::tolower);
    
    // Find "from " keyword
    size_t from_pos = lower_sql.find("from ");
    if (from_pos == std::string::npos) {
        return "unknown_table";
    }
    
    // Skip "from "
    from_pos += 5;
    
    // Skip whitespace
    while (from_pos < lower_sql.length() && std::isspace(lower_sql[from_pos])) {
        from_pos++;
    }
    
    // Find the end of the table name (next whitespace, comma, or end of string)
    size_t end_pos = from_pos;
    while (end_pos < lower_sql.length() && 
           !std::isspace(lower_sql[end_pos]) && 
           lower_sql[end_pos] != ',' &&
           lower_sql[end_pos] != '.' &&
           lower_sql[end_pos] != '(' &&
           lower_sql[end_pos] != ')') {
        end_pos++;
    }
    
    if (end_pos > from_pos) {
        return lower_sql.substr(from_pos, end_pos - from_pos);
    }
    
    return "unknown_table";
}

// Helper function to validate SQL conditions (basic validation)
inline bool validate_sql_condition(const std::string& condition) {
    // Basic validation - ensure it's not empty and doesn't contain dangerous keywords
    if (condition.empty()) return false;
    
    // Convert to lowercase for checking
    std::string lower_condition = condition;
    std::transform(lower_condition.begin(), lower_condition.end(), 
                  lower_condition.begin(), ::tolower);
    
    // Check for dangerous keywords that shouldn't be in RLS conditions
    const std::vector<std::string> dangerous_keywords = {
        "drop ", "delete ", "truncate ", "alter ", "create ", "grant ", "revoke "
    };
    
    for (const auto& keyword : dangerous_keywords) {
        if (lower_condition.find(keyword) != std::string::npos) {
            return false;
        }
    }
    
    return true;
}

// Predefined SQL condition builders for common patterns
namespace rls {

// Check if a column equals the current user's identity
inline std::string user_owns(const std::string& column_name) {
    return column_name + " = current_user_identity()";
}

// Check if a column is in a set of values
inline std::string column_in(const std::string& column_name, const std::vector<std::string>& values) {
    if (values.empty()) return "false";
    
    std::string condition = column_name + " IN (";
    for (size_t i = 0; i < values.size(); ++i) {
        if (i > 0) condition += ", ";
        condition += "'" + values[i] + "'";
    }
    condition += ")";
    return condition;
}

// Check if user has a specific role
inline std::string user_has_role(const std::string& role) {
    return "current_user_has_role('" + role + "')";
}

// Combine conditions with AND
inline std::string and_conditions(const std::vector<std::string>& conditions) {
    if (conditions.empty()) return "true";
    if (conditions.size() == 1) return conditions[0];
    
    std::string result = "(";
    for (size_t i = 0; i < conditions.size(); ++i) {
        if (i > 0) result += " AND ";
        result += "(" + conditions[i] + ")";
    }
    result += ")";
    return result;
}

// Combine conditions with OR
inline std::string or_conditions(const std::vector<std::string>& conditions) {
    if (conditions.empty()) return "false";
    if (conditions.size() == 1) return conditions[0];
    
    std::string result = "(";
    for (size_t i = 0; i < conditions.size(); ++i) {
        if (i > 0) result += " OR ";
        result += "(" + conditions[i] + ")";
    }
    result += ")";
    return result;
}

} // namespace rls

} // namespace SpacetimeDb