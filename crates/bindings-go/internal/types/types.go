package spacetimedb

import (
	"errors"
	"fmt"
	"regexp"
	"runtime/debug"
	"strings"
	"time"
	"unsafe"
)

// TableID represents a unique identifier for a table
type TableID struct {
	id uint32
}

// IndexID represents a unique identifier for an index
type IndexID struct {
	id uint32
}

// RowIter represents an iterator over table rows
type RowIter struct {
	raw uint32
}

// Errno represents a SpacetimeDB error code
type Errno struct {
	code uint16
}

// Constants for error codes
const (
	ErrNoSuchIter       uint16 = 0x0001
	ErrBufferTooSmall   uint16 = 0x0002
	ErrNoSuchTable      uint16 = 0x0003
	ErrNoSuchIndex      uint16 = 0x0004
	ErrWrongIndexAlgo   uint16 = 0x0005
	ErrBsatnDecode      uint16 = 0x0006
	ErrMemoryExhausted  uint16 = 0x0007
	ErrOutOfBounds      uint16 = 0x0008
	ErrNotInTransaction uint16 = 0x0009
	ErrExhausted        uint16 = 0x000A
)

// Constants for inclusive/exclusive bounds
const (
	Exclusive uint32 = 0
	Inclusive uint32 = 1
)

// INVALID represents an invalid RowIter
const INVALID uint32 = 0

// Table represents a database table
type Table interface {
	// GetID returns the table's ID
	GetID() TableID
	// GetName returns the table's name
	GetName() string
	// GetSchema returns the table's schema
	GetSchema() string
	// GetRowCount returns the table's row count
	GetRowCount() (uint64, error)
	// GetIndexCount returns the table's index count
	GetIndexCount() (uint32, error)
	// GetIndexByID returns an index by ID
	GetIndexByID(indexID IndexID) (Index, error)
	// GetIndexByName returns an index by name
	GetIndexByName(name string) (Index, error)
	// Scan scans a table
	Scan(limit uint32, offset uint32) (RowIter, error)
	// ScanIndex scans an index
	ScanIndex(indexID IndexID, lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (RowIter, error)
}

// Index represents a database index
type Index interface {
	// GetID returns the index's ID
	GetID() IndexID
	// GetName returns the index's name
	GetName() string
	// GetTableID returns the ID of the table this index belongs to
	GetTableID() TableID
	// GetAlgorithm returns the index's algorithm
	GetAlgorithm() string
}

// Database represents the main database interface
type Database interface {
	// GetTable returns a table by ID
	GetTable(id TableID) (Table, error)
	// GetTableByName returns a table by name
	GetTableByName(name string) (Table, error)
	// GetIndex returns an index by ID
	GetIndex(id IndexID) (Index, error)
	// GetIndexByName returns an index by name
	GetIndexByName(name string) (Index, error)
	// Insert inserts a row into a table
	Insert(tableID TableID, data []byte) error
	// Update updates rows in a table
	Update(tableID TableID, key []byte, value []byte) error
	// Delete deletes rows from a table
	Delete(tableID TableID, key []byte) error
	// Scan scans a table
	Scan(tableID TableID) (RowIter, error)
	// ScanIndex scans an index
	ScanIndex(indexID IndexID, lower []byte, upper []byte, lowerInclusive bool, upperInclusive bool, limit uint32, offset uint32) (RowIter, error)
}

// NewRowIter creates a new RowIter with the given raw handle
func NewRowIter(raw uint32) *RowIter {
	return &RowIter{raw: raw}
}

// IsExhausted returns whether the iterator is exhausted or not
func (r *RowIter) IsExhausted() bool {
	return (r == nil) || (r.raw == INVALID)
}

// Read reads some number of BSATN-encoded rows into the provided buffer.
// Returns the number of new bytes added to the end of the buffer.
// When the iterator has been exhausted, IsExhausted() will return true.
func (r *RowIter) Read(buf []byte) (int, error) {
	if r.IsExhausted() {
		return 0, nil
	}

	// Get a pointer to the buffer's data
	bufPtr := unsafe.Pointer(&buf[0])
	bufLen := uint32(len(buf))

	// Call the native function to advance the iterator
	ret := rowIterBsatnAdvance(r.raw, bufPtr, &bufLen)

	switch ret {
	case 0: // Success
		return int(bufLen), nil
	case -1: // Exhausted
		r.raw = INVALID
		return int(bufLen), nil
	case -2: // Buffer too small
		return 0, errors.New("buffer too small")
	default:
		return 0, errors.New("unexpected error from row_iter_bsatn_advance")
	}
}

// Close closes the iterator and releases any associated resources
func (r *RowIter) Close() error {
	if !r.IsExhausted() {
		rowIterBsatnClose(r.raw)
		r.raw = INVALID
	}
	return nil
}

// rowIterBsatnAdvance is a native function that advances the iterator and reads data into the buffer
// Returns:
//
//	 0: Success
//	-1: Iterator exhausted
//	-2: Buffer too small
var rowIterBsatnAdvance = func(iter uint32, bufPtr unsafe.Pointer, bufLen *uint32) int32 {
	// This will be implemented by the native binding
	return 0
}

// rowIterBsatnClose is a native function that closes the iterator
var rowIterBsatnClose = func(iter uint32) {
	// This will be implemented by the native binding
}

// NewErrno creates a new Errno with the given error code
func NewErrno(code uint16) *Errno {
	return &Errno{code: code}
}

// Error implements the error interface
func (e *Errno) Error() string {
	return e.String()
}

// Code returns the error code
func (e *Errno) Code() uint16 {
	return e.code
}

// String returns a string representation of the error
func (e *Errno) String() string {
	switch e.code {
	case ErrNoSuchIter:
		return "no such iterator"
	case ErrBufferTooSmall:
		return "buffer too small"
	case ErrNoSuchTable:
		return "no such table"
	case ErrNoSuchIndex:
		return "no such index"
	case ErrWrongIndexAlgo:
		return "wrong index algorithm"
	case ErrBsatnDecode:
		return "BSATN decode error"
	case ErrMemoryExhausted:
		return "memory exhausted"
	case ErrOutOfBounds:
		return "out of bounds"
	case ErrNotInTransaction:
		return "not in transaction"
	case ErrExhausted:
		return "iterator exhausted"
	default:
		return fmt.Sprintf("unknown error code: 0x%04X", e.code)
	}
}

// IsErrno checks if an error is an Errno
func IsErrno(err error) bool {
	_, ok := err.(*Errno)
	return ok
}

// AsErrno converts an error to an Errno if possible
func AsErrno(err error) (*Errno, bool) {
	var e *Errno
	ok := errors.As(err, &e)
	return e, ok
}

// Unwrap returns the underlying error if any
func (e *Errno) Unwrap() error {
	return nil
}

// NewTableID creates a new TableID with the given ID
func NewTableID(id uint32) TableID {
	return TableID{id: id}
}

// ID returns the raw table ID
func (t TableID) ID() uint32 {
	return t.id
}

// String returns a string representation of the table ID
func (t TableID) String() string {
	return fmt.Sprintf("TableID(%d)", t.id)
}

// IsValid returns whether the table ID is valid
func (t TableID) IsValid() bool {
	return t.id != 0
}

// TableIDFromName gets a table ID from a table name
func TableIDFromName(name string) (TableID, error) {
	if name == "" {
		return TableID{}, fmt.Errorf("table name cannot be empty")
	}
	nameBytes := []byte(name)
	var out uint32
	ret := tableIdFromName(nameBytes, uint32(len(nameBytes)), &out)
	if ret != 0 {
		return TableID{}, NewErrno(uint16(ret))
	}
	return TableID{id: out}, nil
}

// NewIndexID creates a new IndexID with the given ID
func NewIndexID(id uint32) IndexID {
	return IndexID{id: id}
}

// ID returns the raw index ID
func (i IndexID) ID() uint32 {
	return i.id
}

// String returns a string representation of the index ID
func (i IndexID) String() string {
	return fmt.Sprintf("IndexID(%d)", i.id)
}

// IsValid returns whether the index ID is valid
func (i IndexID) IsValid() bool {
	return i.id != 0
}

// IndexIDFromName gets an index ID from an index name
func IndexIDFromName(name string) (IndexID, error) {
	if name == "" {
		return IndexID{}, fmt.Errorf("index name cannot be empty")
	}
	nameBytes := []byte(name)
	var out uint32
	ret := indexIdFromName(nameBytes, uint32(len(nameBytes)), &out)
	if ret != 0 {
		return IndexID{}, NewErrno(uint16(ret))
	}
	return IndexID{id: out}, nil
}

// tableIdFromName is a native function that gets a table ID from a name
func tableIdFromName(name []byte, nameLen uint32, out *uint32) int32 {
	// This will be implemented by the native binding
	return 0
}

// indexIdFromName is a native function that gets an index ID from a name
func indexIdFromName(name []byte, nameLen uint32, out *uint32) int32 {
	// This will be implemented by the native binding
	return 0
}

// Package spacetimedb provides Go bindings for SpacetimeDB.
// This package includes core types and functionality for interacting with SpacetimeDB.

// LogLevel represents the severity level of a log message in SpacetimeDB.
// Log levels are ordered from least severe (Trace) to most severe (Fatal).
// Example:
//
//	level := LogLevelInfo
//	if level.AtLeast(LogLevelWarn) {
//	    // Handle warning or more severe messages
//	}
type LogLevel uint8

// Log level constants ordered by severity (least to most severe).
// These constants can be used to control logging verbosity and filter messages.
const (
	// LogLevelTrace is the most verbose logging level.
	// Use for detailed debugging information.
	LogLevelTrace LogLevel = iota

	// LogLevelDebug is used for debugging information.
	// Use for information that is helpful for debugging but not essential.
	LogLevelDebug

	// LogLevelInfo is used for general operational information.
	// Use for normal operational messages that require no action.
	LogLevelInfo

	// LogLevelWarn is used for warning messages.
	// Use for potentially harmful situations that should be addressed.
	LogLevelWarn

	// LogLevelError is used for error messages.
	// Use for error events that might still allow the application to continue running.
	LogLevelError

	// LogLevelFatal is used for fatal error messages.
	// Use for severe error events that will lead to application termination.
	LogLevelFatal
)

// String returns a string representation of the log level.
// The string is always uppercase (e.g., "TRACE", "DEBUG", "INFO").
// For unknown log levels, returns "UNKNOWN(n)" where n is the numeric value.
func (l LogLevel) String() string {
	switch l {
	case LogLevelTrace:
		return "TRACE"
	case LogLevelDebug:
		return "DEBUG"
	case LogLevelInfo:
		return "INFO"
	case LogLevelWarn:
		return "WARN"
	case LogLevelError:
		return "ERROR"
	case LogLevelFatal:
		return "FATAL"
	default:
		return fmt.Sprintf("UNKNOWN(%d)", l)
	}
}

// IsValid returns whether the log level is valid.
// A log level is valid if it is less than or equal to LogLevelFatal.
func (l LogLevel) IsValid() bool {
	return l <= LogLevelFatal
}

// ParseLogLevel converts a string to a LogLevel.
// The string matching is case-insensitive.
// Returns an error if the string does not match any known log level.
// Example:
//
//	level, err := ParseLogLevel("info")
//	if err != nil {
//	    // Handle error
//	}
func ParseLogLevel(s string) (LogLevel, error) {
	switch s {
	case "TRACE", "trace":
		return LogLevelTrace, nil
	case "DEBUG", "debug":
		return LogLevelDebug, nil
	case "INFO", "info":
		return LogLevelInfo, nil
	case "WARN", "warn":
		return LogLevelWarn, nil
	case "ERROR", "error":
		return LogLevelError, nil
	case "FATAL", "fatal":
		return LogLevelFatal, nil
	default:
		return 0, fmt.Errorf("invalid log level: %s", s)
	}
}

// Less returns whether this log level is less severe than the other.
// Example:
//
//	if LogLevelDebug.Less(LogLevelError) {
//	    // Debug is less severe than Error
//	}
func (l LogLevel) Less(other LogLevel) bool {
	return l < other
}

// Greater returns whether this log level is more severe than the other.
// Example:
//
//	if LogLevelError.Greater(LogLevelDebug) {
//	    // Error is more severe than Debug
//	}
func (l LogLevel) Greater(other LogLevel) bool {
	return l > other
}

// AtLeast returns whether this log level is at least as severe as the other.
// Example:
//
//	if level.AtLeast(LogLevelWarn) {
//	    // Handle warning or more severe messages
//	}
func (l LogLevel) AtLeast(other LogLevel) bool {
	return l >= other
}

// AtMost returns whether this log level is at most as severe as the other.
// Example:
//
//	if level.AtMost(LogLevelInfo) {
//	    // Handle info or less severe messages
//	}
func (l LogLevel) AtMost(other LogLevel) bool {
	return l <= other
}

// Type validation errors
var (
	ErrInvalidRowIter  = errors.New("invalid row iterator")
	ErrInvalidTableID  = errors.New("invalid table ID")
	ErrInvalidIndexID  = errors.New("invalid index ID")
	ErrInvalidLogLevel = errors.New("invalid log level")
	ErrNullPointer     = errors.New("null pointer")
	ErrOutOfRange      = errors.New("value out of range")
)

// ValidateRowIter checks if a RowIter is valid
func ValidateRowIter(iter *RowIter) error {
	if iter == nil {
		return ErrNullPointer
	}
	if iter.raw == INVALID {
		return ErrInvalidRowIter
	}
	return nil
}

// ValidateTableID checks if a TableID is valid
func ValidateTableID(id TableID) error {
	if !id.IsValid() {
		return ErrInvalidTableID
	}
	return nil
}

// ValidateIndexID checks if an IndexID is valid
func ValidateIndexID(id IndexID) error {
	if !id.IsValid() {
		return ErrInvalidIndexID
	}
	return nil
}

// ValidateLogLevel checks if a LogLevel is valid
func ValidateLogLevel(level LogLevel) error {
	if !level.IsValid() {
		return ErrInvalidLogLevel
	}
	return nil
}

// ValidateBuffer checks if a buffer is valid for use with native functions
func ValidateBuffer(buf []byte) error {
	if buf == nil {
		return ErrNullPointer
	}
	if len(buf) == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateString checks if a string is valid for use with native functions
func ValidateString(s string) error {
	if s == "" {
		return ErrOutOfRange
	}
	return nil
}

// ValidateUint32 checks if a uint32 is within valid range
func ValidateUint32(n uint32) error {
	if n == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateUint16 checks if a uint16 is within valid range
func ValidateUint16(n uint16) error {
	if n == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateUint8 checks if a uint8 is within valid range
func ValidateUint8(n uint8) error {
	if n == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidatePointer checks if a pointer is valid
func ValidatePointer(ptr unsafe.Pointer) error {
	if ptr == nil {
		return ErrNullPointer
	}
	return nil
}

// ValidateSlice checks if a slice is valid
func ValidateSlice[T any](slice []T) error {
	if slice == nil {
		return ErrNullPointer
	}
	if len(slice) == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateMap checks if a map is valid
func ValidateMap[K comparable, V any](m map[K]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateInterface checks if an interface is valid
func ValidateInterface(i interface{}) error {
	if i == nil {
		return ErrNullPointer
	}
	return nil
}

// ValidateError checks if an error is valid
func ValidateError(err error) error {
	if err == nil {
		return ErrNullPointer
	}
	return nil
}

// ValidateBytes checks if a byte slice is valid
func ValidateBytes(b []byte) error {
	if b == nil {
		return ErrNullPointer
	}
	if len(b) == 0 {
		return ErrOutOfRange
	}
	return nil
}

// ValidateStringSlice checks if a string slice is valid
func ValidateStringSlice(s []string) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, str := range s {
		if err := ValidateString(str); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint32Slice checks if a uint32 slice is valid
func ValidateUint32Slice(s []uint32) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, n := range s {
		if err := ValidateUint32(n); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint16Slice checks if a uint16 slice is valid
func ValidateUint16Slice(s []uint16) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, n := range s {
		if err := ValidateUint16(n); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint8Slice checks if a uint8 slice is valid
func ValidateUint8Slice(s []uint8) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, n := range s {
		if err := ValidateUint8(n); err != nil {
			return err
		}
	}
	return nil
}

// ValidatePointerSlice checks if a pointer slice is valid
func ValidatePointerSlice(s []unsafe.Pointer) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, ptr := range s {
		if err := ValidatePointer(ptr); err != nil {
			return err
		}
	}
	return nil
}

// ValidateInterfaceSlice checks if an interface slice is valid
func ValidateInterfaceSlice(s []interface{}) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, i := range s {
		if err := ValidateInterface(i); err != nil {
			return err
		}
	}
	return nil
}

// ValidateErrorSlice checks if an error slice is valid
func ValidateErrorSlice(s []error) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, err := range s {
		if err := ValidateError(err); err != nil {
			return err
		}
	}
	return nil
}

// ValidateBytesSlice checks if a byte slice slice is valid
func ValidateBytesSlice(s [][]byte) error {
	if s == nil {
		return ErrNullPointer
	}
	if len(s) == 0 {
		return ErrOutOfRange
	}
	for _, b := range s {
		if err := ValidateBytes(b); err != nil {
			return err
		}
	}
	return nil
}

// ValidateStringMap checks if a string map is valid
func ValidateStringMap[V any](m map[string]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateString(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint32Map checks if a uint32 map is valid
func ValidateUint32Map[V any](m map[uint32]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateUint32(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint16Map checks if a uint16 map is valid
func ValidateUint16Map[V any](m map[uint16]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateUint16(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidateUint8Map checks if a uint8 map is valid
func ValidateUint8Map[V any](m map[uint8]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateUint8(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidatePointerMap checks if a pointer map is valid
func ValidatePointerMap[V any](m map[unsafe.Pointer]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidatePointer(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidateInterfaceMap checks if an interface map is valid
func ValidateInterfaceMap[V any](m map[interface{}]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateInterface(k); err != nil {
			return err
		}
	}
	return nil
}

// ValidateErrorMap checks if an error map is valid
func ValidateErrorMap[V any](m map[error]V) error {
	if m == nil {
		return ErrNullPointer
	}
	if len(m) == 0 {
		return ErrOutOfRange
	}
	for k := range m {
		if err := ValidateError(k); err != nil {
			return err
		}
	}
	return nil
}

// Value validation errors
var (
	ErrInvalidRange     = errors.New("value out of valid range")
	ErrInvalidFormat    = errors.New("invalid format")
	ErrInvalidLength    = errors.New("invalid length")
	ErrInvalidCharacter = errors.New("invalid character")
	ErrInvalidValue     = errors.New("invalid value")
)

// ValidateRange checks if a value is within the specified range
func ValidateRange[T interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 | ~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~float32 | ~float64
}](value, min, max T) error {
	if value < min || value > max {
		return fmt.Errorf("%w: %v not in range [%v, %v]", ErrInvalidRange, value, min, max)
	}
	return nil
}

// ValidateLength checks if a string or slice length is within the specified range
func ValidateLength[T string | []byte | []any](value T, min, max int) error {
	length := len(value)
	if length < min || length > max {
		return fmt.Errorf("%w: length %d not in range [%d, %d]", ErrInvalidLength, length, min, max)
	}
	return nil
}

// ValidateFormat checks if a string matches the specified format
func ValidateFormat(s, format string) error {
	matched, err := regexp.MatchString(format, s)
	if err != nil {
		return fmt.Errorf("%w: invalid format pattern: %v", ErrInvalidFormat, err)
	}
	if !matched {
		return fmt.Errorf("%w: string does not match format: %s", ErrInvalidFormat, format)
	}
	return nil
}

// ValidateCharacters checks if a string contains only allowed characters
func ValidateCharacters(s, allowed string) error {
	for _, r := range s {
		if !strings.ContainsRune(allowed, r) {
			return fmt.Errorf("%w: character '%c' not allowed", ErrInvalidCharacter, r)
		}
	}
	return nil
}

// ValidateTableName checks if a table name is valid
func ValidateTableName(name string) error {
	if err := ValidateLength(name, 1, 64); err != nil {
		return err
	}
	// Table names can only contain alphanumeric characters and underscores
	return ValidateFormat(name, `^[a-zA-Z0-9_]+$`)
}

// ValidateIndexName checks if an index name is valid
func ValidateIndexName(name string) error {
	if err := ValidateLength(name, 1, 64); err != nil {
		return err
	}
	// Index names can only contain alphanumeric characters and underscores
	return ValidateFormat(name, `^[a-zA-Z0-9_]+$`)
}

// ValidateBufferSize checks if a buffer size is valid
func ValidateBufferSize(size uint32) error {
	if size == 0 {
		return fmt.Errorf("%w: buffer size cannot be zero", ErrInvalidValue)
	}
	if size > 1024*1024*1024 { // 1GB max
		return fmt.Errorf("%w: buffer size exceeds maximum allowed", ErrInvalidValue)
	}
	return nil
}

// ValidateBSATNData checks if BSATN data is valid
func ValidateBSATNData(data []byte) error {
	if err := ValidateBytes(data); err != nil {
		return err
	}
	if len(data) < 4 { // Minimum BSATN header size
		return fmt.Errorf("%w: BSATN data too short", ErrInvalidValue)
	}
	// Add more BSATN-specific validation as needed
	return nil
}

// ValidateLogMessage checks if a log message is valid
func ValidateLogMessage(msg string) error {
	if err := ValidateLength(msg, 1, 4096); err != nil {
		return err
	}
	// Log messages should not contain control characters
	return ValidateCharacters(msg, " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~\t\n\r")
}

// ValidateErrorCode checks if an error code is valid
func ValidateErrorCode(code uint16) error {
	// Check if the error code is one of the defined constants
	switch code {
	case ErrNoSuchIter, ErrBufferTooSmall, ErrNoSuchTable, ErrNoSuchIndex,
		ErrWrongIndexAlgo, ErrBsatnDecode, ErrMemoryExhausted, ErrOutOfBounds,
		ErrNotInTransaction, ErrExhausted:
		return nil
	default:
		return fmt.Errorf("%w: unknown error code: 0x%04X", ErrInvalidValue, code)
	}
}

// ValidateRowCount checks if a row count is valid
func ValidateRowCount(count uint64) error {
	if count > 1<<32-1 { // Max uint32
		return fmt.Errorf("%w: row count exceeds maximum allowed", ErrInvalidValue)
	}
	return nil
}

// ValidateIndexType checks if an index type is valid
func ValidateIndexType(indexType string) error {
	validTypes := map[string]bool{
		"btree": true,
		"hash":  true,
	}
	if !validTypes[indexType] {
		return fmt.Errorf("%w: invalid index type: %s", ErrInvalidValue, indexType)
	}
	return nil
}

// ValidateTableSchema checks if a table schema is valid
func ValidateTableSchema(schema map[string]string) error {
	if err := ValidateMap(schema); err != nil {
		return err
	}
	for fieldName, fieldType := range schema {
		if err := ValidateTableName(fieldName); err != nil {
			return fmt.Errorf("invalid field name: %w", err)
		}
		if err := ValidateFieldType(fieldType); err != nil {
			return fmt.Errorf("invalid field type: %w", err)
		}
	}
	return nil
}

// ValidateFieldType checks if a field type is valid
func ValidateFieldType(fieldType string) error {
	validTypes := map[string]bool{
		"bool":   true,
		"u8":     true,
		"u16":    true,
		"u32":    true,
		"u64":    true,
		"i8":     true,
		"i16":    true,
		"i32":    true,
		"i64":    true,
		"f32":    true,
		"f64":    true,
		"string": true,
		"bytes":  true,
		"vector": true,
		"option": true,
		"enum":   true,
		"struct": true,
		"union":  true,
	}
	if !validTypes[fieldType] {
		return fmt.Errorf("%w: invalid field type: %s", ErrInvalidValue, fieldType)
	}
	return nil
}

// ValidateIndexSchema checks if an index schema is valid
func ValidateIndexSchema(schema map[string]string) error {
	if err := ValidateMap(schema); err != nil {
		return err
	}
	for fieldName, indexType := range schema {
		if err := ValidateTableName(fieldName); err != nil {
			return fmt.Errorf("invalid field name: %w", err)
		}
		if err := ValidateIndexType(indexType); err != nil {
			return fmt.Errorf("invalid index type: %w", err)
		}
	}
	return nil
}

// ValidateTransactionID checks if a transaction ID is valid
func ValidateTransactionID(id uint64) error {
	if id == 0 {
		return fmt.Errorf("%w: transaction ID cannot be zero", ErrInvalidValue)
	}
	return nil
}

// ValidateModuleName checks if a module name is valid
func ValidateModuleName(name string) error {
	if err := ValidateLength(name, 1, 64); err != nil {
		return err
	}
	// Module names can only contain alphanumeric characters, underscores, and hyphens
	return ValidateFormat(name, `^[a-zA-Z0-9_-]+$`)
}

// ValidateModuleVersion checks if a module version is valid
func ValidateModuleVersion(version string) error {
	if err := ValidateLength(version, 1, 32); err != nil {
		return err
	}
	// Version should follow semantic versioning format
	return ValidateFormat(version, `^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$`)
}

// ValidateModulePath checks if a module path is valid
func ValidateModulePath(path string) error {
	if err := ValidateLength(path, 1, 256); err != nil {
		return err
	}
	// Module paths should be valid file system paths
	return ValidateFormat(path, `^[a-zA-Z0-9/._-]+$`)
}

// ValidateModuleConfig checks if a module configuration is valid
func ValidateModuleConfig(config map[string]interface{}) error {
	if err := ValidateMap(config); err != nil {
		return err
	}
	for key, value := range config {
		if err := ValidateModuleName(key); err != nil {
			return fmt.Errorf("invalid config key: %w", err)
		}
		if err := ValidateConfigValue(value); err != nil {
			return fmt.Errorf("invalid config value: %w", err)
		}
	}
	return nil
}

// ValidateConfigValue checks if a configuration value is valid
func ValidateConfigValue(value interface{}) error {
	switch v := value.(type) {
	case string:
		return ValidateLength(v, 0, 1024)
	case int, int8, int16, int32, int64:
		return nil
	case uint, uint8, uint16, uint32, uint64:
		return nil
	case float32, float64:
		return nil
	case bool:
		return nil
	case []interface{}:
		return ValidateInterfaceSlice(v)
	case map[string]interface{}:
		return ValidateModuleConfig(v)
	case nil:
		return nil
	default:
		return fmt.Errorf("%w: unsupported config value type: %T", ErrInvalidValue, value)
	}
}

// Error handling types and constants
const (
	// Error categories
	ErrCategoryValidation = "validation"
	ErrCategoryRuntime    = "runtime"
	ErrCategorySystem     = "system"
	ErrCategoryWASM       = "wasm"
	ErrCategoryDatabase   = "database"
	ErrCategoryBSATN      = "bsatn"
)

// ErrorContext provides additional context for errors
type ErrorContext struct {
	Category    string
	Operation   string
	Details     map[string]interface{}
	Stack       string
	Timestamp   int64
	Correlation string
}

// SpacetimeError represents a SpacetimeDB error with context
type SpacetimeError struct {
	Errno   *Errno
	Context *ErrorContext
	Cause   error
}

// NewSpacetimeError creates a new SpacetimeError
func NewSpacetimeError(errno *Errno, category, operation string, cause error) *SpacetimeError {
	return &SpacetimeError{
		Errno: errno,
		Context: &ErrorContext{
			Category:  category,
			Operation: operation,
			Details:   make(map[string]interface{}),
			Timestamp: time.Now().UnixNano(),
		},
		Cause: cause,
	}
}

// Error implements the error interface
func (e *SpacetimeError) Error() string {
	if e.Context == nil {
		return e.Errno.Error()
	}
	return fmt.Sprintf("%s: %s", e.Context.Operation, e.Errno.Error())
}

// Unwrap returns the underlying error
func (e *SpacetimeError) Unwrap() error {
	return e.Cause
}

// WithDetails adds additional details to the error context
func (e *SpacetimeError) WithDetails(key string, value interface{}) *SpacetimeError {
	if e.Context == nil {
		e.Context = &ErrorContext{
			Details: make(map[string]interface{}),
		}
	}
	e.Context.Details[key] = value
	return e
}

// WithCorrelation adds a correlation ID to the error context
func (e *SpacetimeError) WithCorrelation(id string) *SpacetimeError {
	if e.Context == nil {
		e.Context = &ErrorContext{
			Details: make(map[string]interface{}),
		}
	}
	e.Context.Correlation = id
	return e
}

// WithStack adds a stack trace to the error context
func (e *SpacetimeError) WithStack() *SpacetimeError {
	if e.Context == nil {
		e.Context = &ErrorContext{
			Details: make(map[string]interface{}),
		}
	}
	e.Context.Stack = string(debug.Stack())
	return e
}

// IsSpacetimeError checks if an error is a SpacetimeError
func IsSpacetimeError(err error) bool {
	_, ok := err.(*SpacetimeError)
	return ok
}

// AsSpacetimeError converts an error to a SpacetimeError if possible
func AsSpacetimeError(err error) (*SpacetimeError, bool) {
	var e *SpacetimeError
	ok := errors.As(err, &e)
	return e, ok
}

// ErrorRecovery provides error recovery mechanisms
type ErrorRecovery struct {
	MaxRetries     int
	RetryDelay     time.Duration
	BackoffFactor  float64
	MaxBackoff     time.Duration
	ErrorHandler   func(error) bool
	RecoveryAction func() error
}

// NewErrorRecovery creates a new ErrorRecovery with default settings
func NewErrorRecovery() *ErrorRecovery {
	return &ErrorRecovery{
		MaxRetries:    3,
		RetryDelay:    time.Second,
		BackoffFactor: 2.0,
		MaxBackoff:    time.Minute,
	}
}

// WithMaxRetries sets the maximum number of retries
func (r *ErrorRecovery) WithMaxRetries(n int) *ErrorRecovery {
	r.MaxRetries = n
	return r
}

// WithRetryDelay sets the initial retry delay
func (r *ErrorRecovery) WithRetryDelay(d time.Duration) *ErrorRecovery {
	r.RetryDelay = d
	return r
}

// WithBackoffFactor sets the backoff factor for retries
func (r *ErrorRecovery) WithBackoffFactor(f float64) *ErrorRecovery {
	r.BackoffFactor = f
	return r
}

// WithMaxBackoff sets the maximum backoff delay
func (r *ErrorRecovery) WithMaxBackoff(d time.Duration) *ErrorRecovery {
	r.MaxBackoff = d
	return r
}

// WithErrorHandler sets a custom error handler
func (r *ErrorRecovery) WithErrorHandler(h func(error) bool) *ErrorRecovery {
	r.ErrorHandler = h
	return r
}

// WithRecoveryAction sets a custom recovery action
func (r *ErrorRecovery) WithRecoveryAction(a func() error) *ErrorRecovery {
	r.RecoveryAction = a
	return r
}

// Execute runs the provided function with retry logic
func (r *ErrorRecovery) Execute(f func() error) error {
	var lastErr error
	delay := r.RetryDelay

	for i := 0; i <= r.MaxRetries; i++ {
		err := f()
		if err == nil {
			return nil
		}

		lastErr = err
		if r.ErrorHandler != nil && !r.ErrorHandler(err) {
			return err
		}

		if i < r.MaxRetries {
			time.Sleep(delay)
			delay = time.Duration(float64(delay) * r.BackoffFactor)
			if delay > r.MaxBackoff {
				delay = r.MaxBackoff
			}

			if r.RecoveryAction != nil {
				if err := r.RecoveryAction(); err != nil {
					return fmt.Errorf("recovery action failed: %w", err)
				}
			}
		}
	}

	return lastErr
}

// ErrorConversion provides utilities for converting between error types
type ErrorConversion struct{}

// FromErrno converts an Errno to a SpacetimeError
func (c *ErrorConversion) FromErrno(errno *Errno, category, operation string) *SpacetimeError {
	return NewSpacetimeError(errno, category, operation, nil)
}

// FromError converts a standard error to a SpacetimeError
func (c *ErrorConversion) FromError(err error, category, operation string) *SpacetimeError {
	if err == nil {
		return nil
	}

	if e, ok := err.(*SpacetimeError); ok {
		return e
	}

	if e, ok := err.(*Errno); ok {
		return NewSpacetimeError(e, category, operation, err)
	}

	return NewSpacetimeError(NewErrno(ErrBsatnDecode), category, operation, err)
}

// ToErrno converts an error to an Errno if possible
func (c *ErrorConversion) ToErrno(err error) (*Errno, bool) {
	if err == nil {
		return nil, false
	}

	if e, ok := err.(*SpacetimeError); ok {
		return e.Errno, true
	}

	if e, ok := err.(*Errno); ok {
		return e, true
	}

	return nil, false
}

// ErrorContext provides utilities for managing error context
type ErrorContextManager struct{}

// NewErrorContext creates a new error context
func (m *ErrorContextManager) NewErrorContext(category, operation string) *ErrorContext {
	return &ErrorContext{
		Category:  category,
		Operation: operation,
		Details:   make(map[string]interface{}),
		Timestamp: time.Now().UnixNano(),
	}
}

// AddDetail adds a detail to the error context
func (m *ErrorContextManager) AddDetail(ctx *ErrorContext, key string, value interface{}) {
	if ctx.Details == nil {
		ctx.Details = make(map[string]interface{})
	}
	ctx.Details[key] = value
}

// AddStack adds a stack trace to the error context
func (m *ErrorContextManager) AddStack(ctx *ErrorContext) {
	ctx.Stack = string(debug.Stack())
}

// AddCorrelation adds a correlation ID to the error context
func (m *ErrorContextManager) AddCorrelation(ctx *ErrorContext, id string) {
	ctx.Correlation = id
}

// GetDetails returns the error context details
func (m *ErrorContextManager) GetDetails(ctx *ErrorContext) map[string]interface{} {
	if ctx == nil {
		return nil
	}
	return ctx.Details
}

// GetStack returns the error context stack trace
func (m *ErrorContextManager) GetStack(ctx *ErrorContext) string {
	if ctx == nil {
		return ""
	}
	return ctx.Stack
}

// GetCorrelation returns the error context correlation ID
func (m *ErrorContextManager) GetCorrelation(ctx *ErrorContext) string {
	if ctx == nil {
		return ""
	}
	return ctx.Correlation
}

// GetTimestamp returns the error context timestamp
func (m *ErrorContextManager) GetTimestamp(ctx *ErrorContext) int64 {
	if ctx == nil {
		return 0
	}
	return ctx.Timestamp
}

// GetCategory returns the error context category
func (m *ErrorContextManager) GetCategory(ctx *ErrorContext) string {
	if ctx == nil {
		return ""
	}
	return ctx.Category
}

// GetOperation returns the error context operation
func (m *ErrorContextManager) GetOperation(ctx *ErrorContext) string {
	if ctx == nil {
		return ""
	}
	return ctx.Operation
}
