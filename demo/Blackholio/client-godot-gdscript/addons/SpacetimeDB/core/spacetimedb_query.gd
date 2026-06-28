## Fluent builder for SpacetimeDB SQL subscription queries.
##
## Constructs [code]SELECT * FROM table WHERE ...[/code] strings with safe
## identifier validation and proper value escaping. Chain [code].where*()[/code]
## calls then pass [method to_sql] to [method SpacetimeDBClient.subscribe].
##
## [b]Usage:[/b]
## [codeblock]
## var sql: String = SpacetimeDBQuery.table("pawn_info").where("owner", identity).to_sql()
## client.subscribe([sql])
## [/codeblock]
class_name SpacetimeDBQuery
extends RefCounted

static var _identifier_regex: RegEx

var _table_name: String
var _conditions: Array[String] = []


## Creates a query targeting [param name].
static func table(name: String) -> SpacetimeDBQuery:
	var validated: String = _validate_identifier(name)
	if validated.is_empty():
		push_error("SpacetimeDBQuery.table: invalid or empty table name '%s'." % name)
		return null
	var q: SpacetimeDBQuery = SpacetimeDBQuery.new()
	q._table_name = validated
	return q


## Creates a query from an existing [_ModuleTable] (uses its internal table name).
static func from(t: _ModuleTable) -> SpacetimeDBQuery:
	var q: SpacetimeDBQuery = SpacetimeDBQuery.new()
	q._table_name = t._table_name
	return q


## Adds [code]field = value[/code]. Multiple conditions are AND'd.
func where(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s = %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field != value[/code].
func where_ne(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s != %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field > value[/code].
func where_gt(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s > %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field < value[/code].
func where_lt(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s < %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field >= value[/code].
func where_gte(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s >= %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field <= value[/code].
func where_lte(field: String, value: Variant) -> SpacetimeDBQuery:
	_conditions.append("%s <= %s" % [_validate_identifier(field), _format_value(value)])
	return self


## Adds [code]field IN (v1, v2, ...)[/code]. Empty [param values] is a no-op
## (an empty IN list is invalid SQL).
func where_in(field: String, values: Array) -> SpacetimeDBQuery:
	if values.is_empty():
		push_error("SpacetimeDBQuery.where_in: empty value list for field '%s'." % field)
		return self
	var formatted: Array[String] = []
	for v: Variant in values:
		formatted.append(_format_value(v))
	_conditions.append("%s IN (%s)" % [_validate_identifier(field), ", ".join(formatted)])
	return self


## Adds an OR group of equality checks: [code](f1 = v1 OR f2 = v2 ...)[/code],
## ANDed with the other conditions. [param pairs] is an [Array] of
## [code][field, value][/code] two-element arrays. Empty [param pairs] is a no-op.
func where_any(pairs: Array) -> SpacetimeDBQuery:
	var ors: Array[String] = []
	for p: Array in pairs:
		if p.size() != 2:
			push_error("SpacetimeDBQuery.where_any: each pair must be [field, value].")
			continue
		ors.append("%s = %s" % [_validate_identifier(p[0]), _format_value(p[1])])
	if not ors.is_empty():
		_conditions.append("(%s)" % " OR ".join(ors))
	return self


## Builds and returns the complete SQL string.
func to_sql() -> String:
	var sql: String = "SELECT * FROM %s" % _table_name
	if not _conditions.is_empty():
		sql += " WHERE " + " AND ".join(_conditions)
	return sql


func _to_string() -> String:
	return to_sql()

# --- Value formatting with proper escaping ---


static func _format_value(value: Variant) -> String:
	var _vt: int = typeof(value)
	if _vt == TYPE_STRING:
		return "'%s'" % value.replace("'", "''")
	elif _vt == TYPE_BOOL:
		return "true" if value else "false"
	elif _vt == TYPE_PACKED_BYTE_ARRAY:
		# SpacetimeDB hex literal: bare 0x... (not a quoted string).
		return "0x%s" % (value as PackedByteArray).hex_encode()
	elif _vt == TYPE_FLOAT:
		var f: float = value
		if is_nan(f):
			push_error("SpacetimeDBQuery: NaN cannot be represented in SQL.")
			return "NULL"
		if is_inf(f):
			push_error("SpacetimeDBQuery: Infinity cannot be represented in SQL.")
			return "NULL"
		# Lossless round-trip for double; locale-independent.
		return "%.17g" % f
	else:
		return str(value)

# --- Identifier validation ---


static func _validate_identifier(name: Variant) -> String:
	var s: String = str(name)
	if _identifier_regex == null:
		_identifier_regex = RegEx.new()
		_identifier_regex.compile("^[a-zA-Z_][a-zA-Z0-9_]*$")
	if not _identifier_regex.search(s):
		push_error("SpacetimeDBQuery: Invalid SQL identifier '%s'. Only alphanumeric characters and underscores are allowed." % s)
		return ""
	return s


## Formats a 32-byte identity as a SpacetimeDB hex literal (e.g. [code]0x...[/code]).
static func identity(bytes: PackedByteArray) -> String:
	return "0x%s" % bytes.hex_encode()
