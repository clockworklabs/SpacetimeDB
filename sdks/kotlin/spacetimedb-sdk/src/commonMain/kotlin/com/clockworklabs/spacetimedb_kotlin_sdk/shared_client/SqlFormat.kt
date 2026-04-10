package com.clockworklabs.spacetimedb_kotlin_sdk.shared_client

/**
 * SQL formatting utilities for the typed query builder.
 * Handles identifier quoting and literal escaping.
 */
@InternalSpacetimeApi
public object SqlFormat {
    /**
     * Quote a SQL identifier with double quotes, escaping internal double quotes by doubling.
     * Example: `tableName` → `"tableName"`, `bad"name` → `"bad""name"`
     */
    public fun quoteIdent(ident: String): String = "\"${ident.replace("\"", "\"\"")}\""

    /**
     * Format a string value as a SQL string literal with single quotes.
     * Internal single quotes are escaped by doubling.
     * Example: `O'Brien` → `'O''Brien'`
     */
    public fun formatStringLiteral(value: String): String = "'${value.replace("'", "''")}'"

    /**
     * Format a hex string as a SQL hex literal.
     * Strips optional `0x` prefix and hyphens, validates all characters are hex digits.
     * Example: `01020304` → `0x01020304`
     */
    public fun formatHexLiteral(hex: String): String {
        var cleaned = hex
        if (cleaned.startsWith("0x", ignoreCase = true)) {
            cleaned = cleaned.substring(2)
        }
        cleaned = cleaned.replace("-", "")
        require(cleaned.isNotEmpty()) { "Empty hex string: $hex" }
        require(cleaned.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) {
            "Invalid hex string: $hex"
        }
        return "0x$cleaned"
    }
}
