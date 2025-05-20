package spacetimedb

import (
	"errors"
	"math"
	"testing"
	"unsafe"
)

// Mock implementation of rowIterBsatnAdvance for testing
func mockRowIterBsatnAdvance(iter uint32, bufPtr unsafe.Pointer, bufLen *uint32) int32 {
	// Simulate different scenarios based on the iterator value
	switch iter {
	case 1: // Valid iterator with data
		*bufLen = 10
		return 0
	case 2: // Exhausted iterator
		*bufLen = 0
		return -1
	case 3: // Buffer too small
		return -2
	default:
		return -1
	}
}

// Mock implementation of rowIterBsatnClose for testing
func mockRowIterBsatnClose(iter uint32) {
	// In a real test, we might want to track if this was called
}

func TestRowIter_Read(t *testing.T) {
	// Replace the native function with our mock for testing
	rowIterBsatnAdvance = mockRowIterBsatnAdvance

	tests := []struct {
		name        string
		iter        *RowIter
		buf         []byte
		wantN       int
		wantErr     bool
		description string
	}{
		{
			name:        "Valid iterator with data",
			iter:        NewRowIter(1),
			buf:         make([]byte, 20),
			wantN:       10,
			wantErr:     false,
			description: "Should successfully read data into buffer",
		},
		{
			name:        "Exhausted iterator",
			iter:        NewRowIter(2),
			buf:         make([]byte, 20),
			wantN:       0,
			wantErr:     false,
			description: "Should handle exhausted iterator gracefully",
		},
		{
			name:        "Buffer too small",
			iter:        NewRowIter(3),
			buf:         make([]byte, 5),
			wantN:       0,
			wantErr:     true,
			description: "Should return error when buffer is too small",
		},
		{
			name:        "Invalid iterator",
			iter:        NewRowIter(INVALID),
			buf:         make([]byte, 20),
			wantN:       0,
			wantErr:     false,
			description: "Should handle invalid iterator gracefully",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotN, err := tt.iter.Read(tt.buf)
			if (err != nil) != tt.wantErr {
				t.Errorf("RowIter.Read() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if gotN != tt.wantN {
				t.Errorf("RowIter.Read() = %v, want %v", gotN, tt.wantN)
			}
		})
	}
}

func TestRowIter_IsExhausted(t *testing.T) {
	tests := []struct {
		name        string
		iter        *RowIter
		want        bool
		description string
	}{
		{
			name:        "Valid iterator",
			iter:        NewRowIter(1),
			want:        false,
			description: "Should return false for valid iterator",
		},
		{
			name:        "Invalid iterator",
			iter:        NewRowIter(INVALID),
			want:        true,
			description: "Should return true for invalid iterator",
		},
		{
			name:        "Nil iterator",
			iter:        nil,
			want:        true,
			description: "Should handle nil iterator gracefully",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.iter.IsExhausted(); got != tt.want {
				t.Errorf("RowIter.IsExhausted() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestRowIter_Close(t *testing.T) {
	// Replace the native function with our mock for testing
	rowIterBsatnClose = mockRowIterBsatnClose

	tests := []struct {
		name        string
		iter        *RowIter
		wantErr     bool
		description string
	}{
		{
			name:        "Valid iterator",
			iter:        NewRowIter(1),
			wantErr:     false,
			description: "Should close valid iterator successfully",
		},
		{
			name:        "Already exhausted iterator",
			iter:        NewRowIter(INVALID),
			wantErr:     false,
			description: "Should handle already exhausted iterator gracefully",
		},
		{
			name:        "Nil iterator",
			iter:        nil,
			wantErr:     false,
			description: "Should handle nil iterator gracefully",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if err := tt.iter.Close(); (err != nil) != tt.wantErr {
				t.Errorf("RowIter.Close() error = %v, wantErr %v", err, tt.wantErr)
			}
			// Verify iterator is marked as exhausted after close
			if !tt.iter.IsExhausted() {
				t.Error("RowIter.Close() did not mark iterator as exhausted")
			}
		})
	}
}

func TestRowIter_INVALID(t *testing.T) {
	// Test that INVALID constant is properly defined
	if INVALID != 0 {
		t.Errorf("INVALID constant = %v, want 0", INVALID)
	}

	// Test that NewRowIter with INVALID creates an exhausted iterator
	iter := NewRowIter(INVALID)
	if !iter.IsExhausted() {
		t.Error("NewRowIter(INVALID) did not create an exhausted iterator")
	}
}

func TestRowIter_Integration(t *testing.T) {
	// Test the complete lifecycle of a RowIter
	iter := NewRowIter(1)
	if iter.IsExhausted() {
		t.Error("New iterator should not be exhausted")
	}

	// Test reading data
	buf := make([]byte, 20)
	n, err := iter.Read(buf)
	if err != nil {
		t.Errorf("Read failed: %v", err)
	}
	if n != 10 {
		t.Errorf("Read returned %d bytes, want 10", n)
	}

	// Test closing
	err = iter.Close()
	if err != nil {
		t.Errorf("Close failed: %v", err)
	}
	if !iter.IsExhausted() {
		t.Error("Iterator should be exhausted after Close")
	}

	// Test reading after close
	n, err = iter.Read(buf)
	if err != nil {
		t.Errorf("Read after close failed: %v", err)
	}
	if n != 0 {
		t.Errorf("Read after close returned %d bytes, want 0", n)
	}
}

func TestErrno_ErrorInterface(t *testing.T) {
	// Test that Errno implements the error interface
	var _ error = (*Errno)(nil)

	tests := []struct {
		name        string
		errno       *Errno
		wantErr     bool
		description string
	}{
		{
			name:        "Valid error code",
			errno:       NewErrno(ErrNoSuchIter),
			wantErr:     true,
			description: "Should implement error interface for valid error code",
		},
		{
			name:        "Zero error code",
			errno:       NewErrno(0),
			wantErr:     true,
			description: "Should implement error interface for zero error code",
		},
		{
			name:        "Unknown error code",
			errno:       NewErrno(0xFFFF),
			wantErr:     true,
			description: "Should implement error interface for unknown error code",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.errno
			if (err != nil) != tt.wantErr {
				t.Errorf("Errno.Error() = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}

func TestErrno_MessageFormatting(t *testing.T) {
	tests := []struct {
		name        string
		errno       *Errno
		want        string
		description string
	}{
		{
			name:        "NoSuchIter error",
			errno:       NewErrno(ErrNoSuchIter),
			want:        "no such iterator",
			description: "Should format NoSuchIter error correctly",
		},
		{
			name:        "BufferTooSmall error",
			errno:       NewErrno(ErrBufferTooSmall),
			want:        "buffer too small",
			description: "Should format BufferTooSmall error correctly",
		},
		{
			name:        "NoSuchTable error",
			errno:       NewErrno(ErrNoSuchTable),
			want:        "no such table",
			description: "Should format NoSuchTable error correctly",
		},
		{
			name:        "Unknown error code",
			errno:       NewErrno(0xFFFF),
			want:        "unknown error code: 0xFFFF",
			description: "Should format unknown error code correctly",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.errno.Error(); got != tt.want {
				t.Errorf("Errno.Error() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestErrno_CodeConversion(t *testing.T) {
	tests := []struct {
		name        string
		code        uint16
		want        uint16
		description string
	}{
		{
			name:        "NoSuchIter code",
			code:        ErrNoSuchIter,
			want:        ErrNoSuchIter,
			description: "Should convert NoSuchIter code correctly",
		},
		{
			name:        "BufferTooSmall code",
			code:        ErrBufferTooSmall,
			want:        ErrBufferTooSmall,
			description: "Should convert BufferTooSmall code correctly",
		},
		{
			name:        "Custom error code",
			code:        0x1234,
			want:        0x1234,
			description: "Should convert custom error code correctly",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			errno := NewErrno(tt.code)
			if got := errno.Code(); got != tt.want {
				t.Errorf("Errno.Code() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestErrno_StringRepresentation(t *testing.T) {
	tests := []struct {
		name        string
		errno       *Errno
		want        string
		description string
	}{
		{
			name:        "NoSuchIter string",
			errno:       NewErrno(ErrNoSuchIter),
			want:        "no such iterator",
			description: "Should represent NoSuchIter as string correctly",
		},
		{
			name:        "BufferTooSmall string",
			errno:       NewErrno(ErrBufferTooSmall),
			want:        "buffer too small",
			description: "Should represent BufferTooSmall as string correctly",
		},
		{
			name:        "NoSuchTable string",
			errno:       NewErrno(ErrNoSuchTable),
			want:        "no such table",
			description: "Should represent NoSuchTable as string correctly",
		},
		{
			name:        "Unknown error string",
			errno:       NewErrno(0xFFFF),
			want:        "unknown error code: 0xFFFF",
			description: "Should represent unknown error as string correctly",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.errno.String(); got != tt.want {
				t.Errorf("Errno.String() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestErrno_ErrorHelpers(t *testing.T) {
	tests := []struct {
		name        string
		err         error
		wantIsErrno bool
		wantErrno   *Errno
		description string
	}{
		{
			name:        "Valid Errno",
			err:         NewErrno(ErrNoSuchIter),
			wantIsErrno: true,
			wantErrno:   NewErrno(ErrNoSuchIter),
			description: "Should identify valid Errno",
		},
		{
			name:        "Standard error",
			err:         errors.New("standard error"),
			wantIsErrno: false,
			wantErrno:   nil,
			description: "Should not identify standard error as Errno",
		},
		{
			name:        "Nil error",
			err:         nil,
			wantIsErrno: false,
			wantErrno:   nil,
			description: "Should handle nil error",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Test IsErrno
			if got := IsErrno(tt.err); got != tt.wantIsErrno {
				t.Errorf("IsErrno() = %v, want %v", got, tt.wantIsErrno)
			}

			// Test AsErrno
			if got, ok := AsErrno(tt.err); ok != tt.wantIsErrno {
				t.Errorf("AsErrno() ok = %v, want %v", ok, tt.wantIsErrno)
			} else if tt.wantIsErrno && (got == nil || got.Code() != tt.wantErrno.Code()) {
				t.Errorf("AsErrno() = %v, want %v", got, tt.wantErrno)
			}
		})
	}
}

func TestErrno_Unwrap(t *testing.T) {
	tests := []struct {
		name        string
		errno       *Errno
		want        error
		description string
	}{
		{
			name:        "Standard Errno",
			errno:       NewErrno(ErrNoSuchIter),
			want:        nil,
			description: "Should return nil for standard Errno",
		},
		{
			name:        "Custom Errno",
			errno:       NewErrno(0xFFFF),
			want:        nil,
			description: "Should return nil for custom Errno",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.errno.Unwrap(); got != tt.want {
				t.Errorf("Errno.Unwrap() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestTableID_Basic(t *testing.T) {
	tests := []struct {
		name        string
		id          uint32
		wantID      uint32
		wantValid   bool
		wantString  string
		description string
	}{
		{
			name:        "Valid ID",
			id:          123,
			wantID:      123,
			wantValid:   true,
			wantString:  "TableID(123)",
			description: "Should create a valid TableID with correct ID",
		},
		{
			name:        "Zero ID",
			id:          0,
			wantID:      0,
			wantValid:   false,
			wantString:  "TableID(0)",
			description: "Should create an invalid TableID with zero ID",
		},
		{
			name:        "Max ID",
			id:          math.MaxUint32,
			wantID:      math.MaxUint32,
			wantValid:   true,
			wantString:  "TableID(4294967295)",
			description: "Should create a valid TableID with maximum ID",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tableID := NewTableID(tt.id)
			if got := tableID.ID(); got != tt.wantID {
				t.Errorf("TableID.ID() = %v, want %v", got, tt.wantID)
			}
			if got := tableID.IsValid(); got != tt.wantValid {
				t.Errorf("TableID.IsValid() = %v, want %v", got, tt.wantValid)
			}
			if got := tableID.String(); got != tt.wantString {
				t.Errorf("TableID.String() = %v, want %v", got, tt.wantString)
			}
		})
	}
}

func TestTableID_FromName(t *testing.T) {
	// Since we can't mock the native functions directly, we'll test the error handling
	// and basic functionality of TableIDFromName
	tests := []struct {
		name        string
		tableName   string
		wantErr     bool
		description string
	}{
		{
			name:        "Empty table name",
			tableName:   "",
			wantErr:     true,
			description: "Should return error for empty table name",
		},
		{
			name:        "Non-empty table name",
			tableName:   "test_table",
			wantErr:     false,
			description: "Should handle non-empty table name",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := TableIDFromName(tt.tableName)
			if (err != nil) != tt.wantErr {
				t.Errorf("TableIDFromName() error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}

func TestIndexID_Basic(t *testing.T) {
	tests := []struct {
		name        string
		id          uint32
		wantID      uint32
		wantValid   bool
		wantString  string
		description string
	}{
		{
			name:        "Valid ID",
			id:          456,
			wantID:      456,
			wantValid:   true,
			wantString:  "IndexID(456)",
			description: "Should create a valid IndexID with correct ID",
		},
		{
			name:        "Zero ID",
			id:          0,
			wantID:      0,
			wantValid:   false,
			wantString:  "IndexID(0)",
			description: "Should create an invalid IndexID with zero ID",
		},
		{
			name:        "Max ID",
			id:          math.MaxUint32,
			wantID:      math.MaxUint32,
			wantValid:   true,
			wantString:  "IndexID(4294967295)",
			description: "Should create a valid IndexID with maximum ID",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			indexID := NewIndexID(tt.id)
			if got := indexID.ID(); got != tt.wantID {
				t.Errorf("IndexID.ID() = %v, want %v", got, tt.wantID)
			}
			if got := indexID.IsValid(); got != tt.wantValid {
				t.Errorf("IndexID.IsValid() = %v, want %v", got, tt.wantValid)
			}
			if got := indexID.String(); got != tt.wantString {
				t.Errorf("IndexID.String() = %v, want %v", got, tt.wantString)
			}
		})
	}
}

func TestIndexID_FromName(t *testing.T) {
	// Since we can't mock the native functions directly, we'll test the error handling
	// and basic functionality of IndexIDFromName
	tests := []struct {
		name        string
		indexName   string
		wantErr     bool
		description string
	}{
		{
			name:        "Empty index name",
			indexName:   "",
			wantErr:     true,
			description: "Should return error for empty index name",
		},
		{
			name:        "Non-empty index name",
			indexName:   "test_index",
			wantErr:     false,
			description: "Should handle non-empty index name",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := IndexIDFromName(tt.indexName)
			if (err != nil) != tt.wantErr {
				t.Errorf("IndexIDFromName() error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}

func TestTableID_IndexID_Integration(t *testing.T) {
	// Test that TableID and IndexID can be used together
	tableID := NewTableID(123)
	indexID := NewIndexID(456)

	if !tableID.IsValid() || !indexID.IsValid() {
		t.Error("TableID and IndexID should be valid")
	}

	if tableID.ID() != 123 || indexID.ID() != 456 {
		t.Error("TableID and IndexID should have correct IDs")
	}

	// Test string representations
	tableStr := tableID.String()
	indexStr := indexID.String()
	if tableStr != "TableID(123)" || indexStr != "IndexID(456)" {
		t.Error("TableID and IndexID should have correct string representations")
	}
}

func TestLogLevel_String(t *testing.T) {
	tests := []struct {
		name        string
		level       LogLevel
		want        string
		description string
	}{
		{
			name:        "Trace level",
			level:       LogLevelTrace,
			want:        "TRACE",
			description: "Should return TRACE for trace level",
		},
		{
			name:        "Debug level",
			level:       LogLevelDebug,
			want:        "DEBUG",
			description: "Should return DEBUG for debug level",
		},
		{
			name:        "Info level",
			level:       LogLevelInfo,
			want:        "INFO",
			description: "Should return INFO for info level",
		},
		{
			name:        "Warn level",
			level:       LogLevelWarn,
			want:        "WARN",
			description: "Should return WARN for warn level",
		},
		{
			name:        "Error level",
			level:       LogLevelError,
			want:        "ERROR",
			description: "Should return ERROR for error level",
		},
		{
			name:        "Fatal level",
			level:       LogLevelFatal,
			want:        "FATAL",
			description: "Should return FATAL for fatal level",
		},
		{
			name:        "Unknown level",
			level:       LogLevel(99),
			want:        "UNKNOWN(99)",
			description: "Should return UNKNOWN(n) for unknown level",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.level.String(); got != tt.want {
				t.Errorf("LogLevel.String() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestLogLevel_IsValid(t *testing.T) {
	tests := []struct {
		name        string
		level       LogLevel
		want        bool
		description string
	}{
		{
			name:        "Trace level",
			level:       LogLevelTrace,
			want:        true,
			description: "Should validate trace level",
		},
		{
			name:        "Debug level",
			level:       LogLevelDebug,
			want:        true,
			description: "Should validate debug level",
		},
		{
			name:        "Info level",
			level:       LogLevelInfo,
			want:        true,
			description: "Should validate info level",
		},
		{
			name:        "Warn level",
			level:       LogLevelWarn,
			want:        true,
			description: "Should validate warn level",
		},
		{
			name:        "Error level",
			level:       LogLevelError,
			want:        true,
			description: "Should validate error level",
		},
		{
			name:        "Fatal level",
			level:       LogLevelFatal,
			want:        true,
			description: "Should validate fatal level",
		},
		{
			name:        "Invalid level",
			level:       LogLevel(99),
			want:        false,
			description: "Should not validate invalid level",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.level.IsValid(); got != tt.want {
				t.Errorf("LogLevel.IsValid() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestParseLogLevel(t *testing.T) {
	tests := []struct {
		name        string
		input       string
		want        LogLevel
		wantErr     bool
		description string
	}{
		{
			name:        "Trace level uppercase",
			input:       "TRACE",
			want:        LogLevelTrace,
			wantErr:     false,
			description: "Should parse TRACE level",
		},
		{
			name:        "Trace level lowercase",
			input:       "trace",
			want:        LogLevelTrace,
			wantErr:     false,
			description: "Should parse trace level",
		},
		{
			name:        "Debug level uppercase",
			input:       "DEBUG",
			want:        LogLevelDebug,
			wantErr:     false,
			description: "Should parse DEBUG level",
		},
		{
			name:        "Debug level lowercase",
			input:       "debug",
			want:        LogLevelDebug,
			wantErr:     false,
			description: "Should parse debug level",
		},
		{
			name:        "Info level uppercase",
			input:       "INFO",
			want:        LogLevelInfo,
			wantErr:     false,
			description: "Should parse INFO level",
		},
		{
			name:        "Info level lowercase",
			input:       "info",
			want:        LogLevelInfo,
			wantErr:     false,
			description: "Should parse info level",
		},
		{
			name:        "Warn level uppercase",
			input:       "WARN",
			want:        LogLevelWarn,
			wantErr:     false,
			description: "Should parse WARN level",
		},
		{
			name:        "Warn level lowercase",
			input:       "warn",
			want:        LogLevelWarn,
			wantErr:     false,
			description: "Should parse warn level",
		},
		{
			name:        "Error level uppercase",
			input:       "ERROR",
			want:        LogLevelError,
			wantErr:     false,
			description: "Should parse ERROR level",
		},
		{
			name:        "Error level lowercase",
			input:       "error",
			want:        LogLevelError,
			wantErr:     false,
			description: "Should parse error level",
		},
		{
			name:        "Fatal level uppercase",
			input:       "FATAL",
			want:        LogLevelFatal,
			wantErr:     false,
			description: "Should parse FATAL level",
		},
		{
			name:        "Fatal level lowercase",
			input:       "fatal",
			want:        LogLevelFatal,
			wantErr:     false,
			description: "Should parse fatal level",
		},
		{
			name:        "Invalid level",
			input:       "INVALID",
			want:        0,
			wantErr:     true,
			description: "Should return error for invalid level",
		},
		{
			name:        "Empty string",
			input:       "",
			want:        0,
			wantErr:     true,
			description: "Should return error for empty string",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseLogLevel(tt.input)
			if (err != nil) != tt.wantErr {
				t.Errorf("ParseLogLevel() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if !tt.wantErr && got != tt.want {
				t.Errorf("ParseLogLevel() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestLogLevel_Comparison(t *testing.T) {
	tests := []struct {
		name        string
		level       LogLevel
		other       LogLevel
		wantLess    bool
		wantGreater bool
		wantAtLeast bool
		wantAtMost  bool
		description string
	}{
		{
			name:        "Trace vs Debug",
			level:       LogLevelTrace,
			other:       LogLevelDebug,
			wantLess:    true,
			wantGreater: false,
			wantAtLeast: false,
			wantAtMost:  true,
			description: "Should compare Trace and Debug correctly",
		},
		{
			name:        "Debug vs Info",
			level:       LogLevelDebug,
			other:       LogLevelInfo,
			wantLess:    true,
			wantGreater: false,
			wantAtLeast: false,
			wantAtMost:  true,
			description: "Should compare Debug and Info correctly",
		},
		{
			name:        "Info vs Warn",
			level:       LogLevelInfo,
			other:       LogLevelWarn,
			wantLess:    true,
			wantGreater: false,
			wantAtLeast: false,
			wantAtMost:  true,
			description: "Should compare Info and Warn correctly",
		},
		{
			name:        "Warn vs Error",
			level:       LogLevelWarn,
			other:       LogLevelError,
			wantLess:    true,
			wantGreater: false,
			wantAtLeast: false,
			wantAtMost:  true,
			description: "Should compare Warn and Error correctly",
		},
		{
			name:        "Error vs Fatal",
			level:       LogLevelError,
			other:       LogLevelFatal,
			wantLess:    true,
			wantGreater: false,
			wantAtLeast: false,
			wantAtMost:  true,
			description: "Should compare Error and Fatal correctly",
		},
		{
			name:        "Equal levels",
			level:       LogLevelInfo,
			other:       LogLevelInfo,
			wantLess:    false,
			wantGreater: false,
			wantAtLeast: true,
			wantAtMost:  true,
			description: "Should compare equal levels correctly",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.level.Less(tt.other); got != tt.wantLess {
				t.Errorf("LogLevel.Less() = %v, want %v", got, tt.wantLess)
			}
			if got := tt.level.Greater(tt.other); got != tt.wantGreater {
				t.Errorf("LogLevel.Greater() = %v, want %v", got, tt.wantGreater)
			}
			if got := tt.level.AtLeast(tt.other); got != tt.wantAtLeast {
				t.Errorf("LogLevel.AtLeast() = %v, want %v", got, tt.wantAtLeast)
			}
			if got := tt.level.AtMost(tt.other); got != tt.wantAtMost {
				t.Errorf("LogLevel.AtMost() = %v, want %v", got, tt.wantAtMost)
			}
		})
	}
}

func TestLogLevel_Validation(t *testing.T) {
	tests := []struct {
		name        string
		level       LogLevel
		wantErr     bool
		description string
	}{
		{
			name:        "Valid Trace level",
			level:       LogLevelTrace,
			wantErr:     false,
			description: "Should validate Trace level",
		},
		{
			name:        "Valid Debug level",
			level:       LogLevelDebug,
			wantErr:     false,
			description: "Should validate Debug level",
		},
		{
			name:        "Valid Info level",
			level:       LogLevelInfo,
			wantErr:     false,
			description: "Should validate Info level",
		},
		{
			name:        "Valid Warn level",
			level:       LogLevelWarn,
			wantErr:     false,
			description: "Should validate Warn level",
		},
		{
			name:        "Valid Error level",
			level:       LogLevelError,
			wantErr:     false,
			description: "Should validate Error level",
		},
		{
			name:        "Valid Fatal level",
			level:       LogLevelFatal,
			wantErr:     false,
			description: "Should validate Fatal level",
		},
		{
			name:        "Invalid level",
			level:       LogLevel(99),
			wantErr:     true,
			description: "Should not validate invalid level",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateLogLevel(tt.level)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateLogLevel() error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}
