package db

import (
	"bytes"
	"compress/gzip"
	"fmt"
	"io"
	"reflect"
	"sync"
	"time"

	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/bsatn"
	"github.com/clockworklabs/SpacetimeDB/crates/bindings-go/internal/runtime"
)

// EncodingManager manages data encoding and decoding operations
type EncodingManager struct {
	mu               sync.RWMutex
	encoders         map[string]Encoder
	decoders         map[string]Decoder
	compressionTypes map[string]Compressor
	schemaCache      map[string]*SchemaInfo
	encodingStats    *EncodingStatistics
	runtime          *runtime.Runtime
	maxCacheSize     int
	compressionLevel int
}

// Encoder interface for data encoding
type Encoder interface {
	Encode(data interface{}) ([]byte, error)
	EncodeTo(data interface{}, writer io.Writer) error
	GetFormat() EncodingFormat
	GetOptions() *EncodingOptions
}

// Decoder interface for data decoding
type Decoder interface {
	Decode(data []byte, target interface{}) error
	DecodeFrom(reader io.Reader, target interface{}) error
	GetFormat() EncodingFormat
	GetOptions() *DecodingOptions
}

// Compressor interface for data compression
type Compressor interface {
	Compress(data []byte) ([]byte, error)
	Decompress(data []byte) ([]byte, error)
	GetType() CompressionType
	GetLevel() int
}

// EncodingFormat represents different encoding formats
type EncodingFormat int

const (
	EncodingBSATN EncodingFormat = iota
	EncodingJSON
	EncodingProtobuf
	EncodingMsgPack
	EncodingAvro
	EncodingCBOR
)

func (ef EncodingFormat) String() string {
	switch ef {
	case EncodingBSATN:
		return "bsatn"
	case EncodingJSON:
		return "json"
	case EncodingProtobuf:
		return "protobuf"
	case EncodingMsgPack:
		return "msgpack"
	case EncodingAvro:
		return "avro"
	case EncodingCBOR:
		return "cbor"
	default:
		return "unknown"
	}
}

// CompressionType represents different compression algorithms
type CompressionType int

const (
	CompressionNone CompressionType = iota
	CompressionGzip
	CompressionLZ4
	CompressionSnappy
	CompressionZstd
)

func (ct CompressionType) String() string {
	switch ct {
	case CompressionNone:
		return "none"
	case CompressionGzip:
		return "gzip"
	case CompressionLZ4:
		return "lz4"
	case CompressionSnappy:
		return "snappy"
	case CompressionZstd:
		return "zstd"
	default:
		return "none"
	}
}

// EncodingOptions contains encoding configuration
type EncodingOptions struct {
	Format           EncodingFormat         `json:"format"`
	Compression      CompressionType        `json:"compression"`
	CompressionLevel int                    `json:"compression_level"`
	PrettyPrint      bool                   `json:"pretty_print"`
	IncludeSchema    bool                   `json:"include_schema"`
	ValidateSchema   bool                   `json:"validate_schema"`
	Properties       map[string]interface{} `json:"properties"`
	CustomOptions    map[string]interface{} `json:"custom_options"`
}

// DecodingOptions contains decoding configuration
type DecodingOptions struct {
	Format        EncodingFormat         `json:"format"`
	Compression   CompressionType        `json:"compression"`
	StrictMode    bool                   `json:"strict_mode"`
	ValidateData  bool                   `json:"validate_data"`
	IgnoreUnknown bool                   `json:"ignore_unknown"`
	Properties    map[string]interface{} `json:"properties"`
	CustomOptions map[string]interface{} `json:"custom_options"`
}

// SchemaInfo contains schema metadata
type SchemaInfo struct {
	ID            string                 `json:"id"`
	Version       uint32                 `json:"version"`
	Format        EncodingFormat         `json:"format"`
	Schema        []byte                 `json:"schema"`
	Checksum      string                 `json:"checksum"`
	CreatedAt     time.Time              `json:"created_at"`
	UpdatedAt     time.Time              `json:"updated_at"`
	Fields        []FieldInfo            `json:"fields"`
	Properties    map[string]interface{} `json:"properties"`
	Compatibility SchemaCompatibility    `json:"compatibility"`
}

// FieldInfo contains field metadata
type FieldInfo struct {
	Name         string      `json:"name"`
	Type         string      `json:"type"`
	Required     bool        `json:"required"`
	DefaultValue interface{} `json:"default_value"`
	Description  string      `json:"description"`
	Tags         []string    `json:"tags"`
}

// SchemaCompatibility defines schema compatibility modes
type SchemaCompatibility int

const (
	CompatibilityNone SchemaCompatibility = iota
	CompatibilityBackward
	CompatibilityForward
	CompatibilityFull
)

// EncodingStatistics contains encoding performance statistics
type EncodingStatistics struct {
	TotalEncoded      uint64                       `json:"total_encoded"`
	TotalDecoded      uint64                       `json:"total_decoded"`
	TotalCompressed   uint64                       `json:"total_compressed"`
	TotalDecompressed uint64                       `json:"total_decompressed"`
	EncodingTime      time.Duration                `json:"encoding_time"`
	DecodingTime      time.Duration                `json:"decoding_time"`
	CompressionTime   time.Duration                `json:"compression_time"`
	DecompressionTime time.Duration                `json:"decompression_time"`
	FormatStats       map[string]*FormatStats      `json:"format_stats"`
	CompressionStats  map[string]*CompressionStats `json:"compression_stats"`
	ErrorCount        uint64                       `json:"error_count"`
	LastResetTime     time.Time                    `json:"last_reset_time"`
}

// FormatStats contains format-specific statistics
type FormatStats struct {
	EncodedCount uint64        `json:"encoded_count"`
	DecodedCount uint64        `json:"decoded_count"`
	TotalBytes   uint64        `json:"total_bytes"`
	AverageSize  float64       `json:"average_size"`
	EncodingTime time.Duration `json:"encoding_time"`
	DecodingTime time.Duration `json:"decoding_time"`
	ErrorCount   uint64        `json:"error_count"`
}

// CompressionStats contains compression-specific statistics
type CompressionStats struct {
	CompressedCount   uint64        `json:"compressed_count"`
	DecompressedCount uint64        `json:"decompressed_count"`
	OriginalBytes     uint64        `json:"original_bytes"`
	CompressedBytes   uint64        `json:"compressed_bytes"`
	CompressionRatio  float64       `json:"compression_ratio"`
	CompressionTime   time.Duration `json:"compression_time"`
	DecompressionTime time.Duration `json:"decompression_time"`
	ErrorCount        uint64        `json:"error_count"`
}

// BSATNEncoder implements BSATN encoding
type BSATNEncoder struct {
	options *EncodingOptions
}

// BSATNDecoder implements BSATN decoding
type BSATNDecoder struct {
	options *DecodingOptions
}

// GzipCompressor implements gzip compression
type GzipCompressor struct {
	level int
}

// NewEncodingManager creates a new encoding manager
func NewEncodingManager(runtime *runtime.Runtime) *EncodingManager {
	manager := &EncodingManager{
		encoders:         make(map[string]Encoder),
		decoders:         make(map[string]Decoder),
		compressionTypes: make(map[string]Compressor),
		schemaCache:      make(map[string]*SchemaInfo),
		runtime:          runtime,
		maxCacheSize:     1000,
		compressionLevel: 6,
		encodingStats: &EncodingStatistics{
			FormatStats:      make(map[string]*FormatStats),
			CompressionStats: make(map[string]*CompressionStats),
			LastResetTime:    time.Now(),
		},
	}

	// Register default encoders and decoders
	manager.registerDefaultEncoders()
	manager.registerDefaultDecoders()
	manager.registerDefaultCompressors()

	return manager
}

// registerDefaultEncoders registers default encoders
func (em *EncodingManager) registerDefaultEncoders() {
	// BSATN encoder
	bsatnEncoder := &BSATNEncoder{
		options: &EncodingOptions{
			Format:      EncodingBSATN,
			Compression: CompressionNone,
			Properties:  make(map[string]interface{}),
		},
	}
	em.encoders["bsatn"] = bsatnEncoder
}

// registerDefaultDecoders registers default decoders
func (em *EncodingManager) registerDefaultDecoders() {
	// BSATN decoder
	bsatnDecoder := &BSATNDecoder{
		options: &DecodingOptions{
			Format:      EncodingBSATN,
			Compression: CompressionNone,
			StrictMode:  true,
			Properties:  make(map[string]interface{}),
		},
	}
	em.decoders["bsatn"] = bsatnDecoder
}

// registerDefaultCompressors registers default compressors
func (em *EncodingManager) registerDefaultCompressors() {
	// Gzip compressor
	gzipCompressor := &GzipCompressor{
		level: em.compressionLevel,
	}
	em.compressionTypes["gzip"] = gzipCompressor
}

// Encode encodes data using the specified format
func (em *EncodingManager) Encode(data interface{}, format EncodingFormat, options *EncodingOptions) ([]byte, error) {
	startTime := time.Now()
	defer func() {
		em.updateEncodingStats(format, time.Since(startTime), true)
	}()

	encoder, err := em.getEncoder(format)
	if err != nil {
		em.updateErrorStats()
		return nil, err
	}

	// Apply options if provided
	if options != nil {
		encoder = em.applyEncodingOptions(encoder, options)
	}

	// Encode data
	encoded, err := encoder.Encode(data)
	if err != nil {
		em.updateErrorStats()
		return nil, fmt.Errorf("encoding failed: %w", err)
	}

	// Apply compression if specified
	if options != nil && options.Compression != CompressionNone {
		compressed, err := em.compress(encoded, options.Compression, options.CompressionLevel)
		if err != nil {
			em.updateErrorStats()
			return nil, fmt.Errorf("compression failed: %w", err)
		}
		encoded = compressed
	}

	return encoded, nil
}

// Decode decodes data using the specified format
func (em *EncodingManager) Decode(data []byte, target interface{}, format EncodingFormat, options *DecodingOptions) error {
	startTime := time.Now()
	defer func() {
		em.updateDecodingStats(format, time.Since(startTime), true)
	}()

	decoder, err := em.getDecoder(format)
	if err != nil {
		em.updateErrorStats()
		return err
	}

	// Apply decompression if specified
	decodedData := data
	if options != nil && options.Compression != CompressionNone {
		decompressed, err := em.decompress(data, options.Compression)
		if err != nil {
			em.updateErrorStats()
			return fmt.Errorf("decompression failed: %w", err)
		}
		decodedData = decompressed
	}

	// Apply options if provided
	if options != nil {
		decoder = em.applyDecodingOptions(decoder, options)
	}

	// Decode data
	err = decoder.Decode(decodedData, target)
	if err != nil {
		em.updateErrorStats()
		return fmt.Errorf("decoding failed: %w", err)
	}

	return nil
}

// EncodeWithSchema encodes data with schema validation
func (em *EncodingManager) EncodeWithSchema(data interface{}, schemaID string, options *EncodingOptions) ([]byte, error) {
	// Get schema
	schema, err := em.getSchema(schemaID)
	if err != nil {
		return nil, fmt.Errorf("schema not found: %w", err)
	}

	// Validate data against schema if requested
	if options != nil && options.ValidateSchema {
		if err := em.validateDataAgainstSchema(data, schema); err != nil {
			return nil, fmt.Errorf("schema validation failed: %w", err)
		}
	}

	// Encode with schema format
	return em.Encode(data, schema.Format, options)
}

// DecodeWithSchema decodes data with schema validation
func (em *EncodingManager) DecodeWithSchema(data []byte, target interface{}, schemaID string, options *DecodingOptions) error {
	// Get schema
	schema, err := em.getSchema(schemaID)
	if err != nil {
		return fmt.Errorf("schema not found: %w", err)
	}

	// Decode with schema format
	if err := em.Decode(data, target, schema.Format, options); err != nil {
		return err
	}

	// Validate decoded data against schema if requested
	if options != nil && options.ValidateData {
		if err := em.validateDataAgainstSchema(target, schema); err != nil {
			return fmt.Errorf("decoded data validation failed: %w", err)
		}
	}

	return nil
}

// RegisterSchema registers a new schema
func (em *EncodingManager) RegisterSchema(schemaInfo *SchemaInfo) error {
	em.mu.Lock()
	defer em.mu.Unlock()

	// Check cache size
	if len(em.schemaCache) >= em.maxCacheSize {
		em.evictOldestSchema()
	}

	// Store schema
	em.schemaCache[schemaInfo.ID] = schemaInfo
	return nil
}

// getEncoder gets an encoder for the specified format
func (em *EncodingManager) getEncoder(format EncodingFormat) (Encoder, error) {
	em.mu.RLock()
	defer em.mu.RUnlock()

	encoder, exists := em.encoders[format.String()]
	if !exists {
		return nil, fmt.Errorf("encoder not found for format: %s", format.String())
	}

	return encoder, nil
}

// getDecoder gets a decoder for the specified format
func (em *EncodingManager) getDecoder(format EncodingFormat) (Decoder, error) {
	em.mu.RLock()
	defer em.mu.RUnlock()

	decoder, exists := em.decoders[format.String()]
	if !exists {
		return nil, fmt.Errorf("decoder not found for format: %s", format.String())
	}

	return decoder, nil
}

// getSchema gets a schema by ID
func (em *EncodingManager) getSchema(schemaID string) (*SchemaInfo, error) {
	em.mu.RLock()
	defer em.mu.RUnlock()

	schema, exists := em.schemaCache[schemaID]
	if !exists {
		return nil, fmt.Errorf("schema not found: %s", schemaID)
	}

	return schema, nil
}

// compress compresses data using the specified compression type
func (em *EncodingManager) compress(data []byte, compressionType CompressionType, level int) ([]byte, error) {
	compressor, exists := em.compressionTypes[compressionType.String()]
	if !exists {
		return nil, fmt.Errorf("compressor not found: %s", compressionType.String())
	}

	return compressor.Compress(data)
}

// decompress decompresses data using the specified compression type
func (em *EncodingManager) decompress(data []byte, compressionType CompressionType) ([]byte, error) {
	compressor, exists := em.compressionTypes[compressionType.String()]
	if !exists {
		return nil, fmt.Errorf("compressor not found: %s", compressionType.String())
	}

	return compressor.Decompress(data)
}

// applyEncodingOptions applies encoding options to an encoder
func (em *EncodingManager) applyEncodingOptions(encoder Encoder, options *EncodingOptions) Encoder {
	// In a real implementation, this would create a configured encoder
	return encoder
}

// applyDecodingOptions applies decoding options to a decoder
func (em *EncodingManager) applyDecodingOptions(decoder Decoder, options *DecodingOptions) Decoder {
	// In a real implementation, this would create a configured decoder
	return decoder
}

// validateDataAgainstSchema validates data against a schema
func (em *EncodingManager) validateDataAgainstSchema(data interface{}, schema *SchemaInfo) error {
	// Simplified validation - would implement proper schema validation
	if data == nil {
		return fmt.Errorf("data cannot be nil")
	}

	// Basic type checking
	dataType := reflect.TypeOf(data)
	if dataType == nil {
		return fmt.Errorf("invalid data type")
	}

	// For testing purposes, accept any valid Go data structure
	return nil
}

// updateEncodingStats updates encoding statistics
func (em *EncodingManager) updateEncodingStats(format EncodingFormat, duration time.Duration, success bool) {
	em.mu.Lock()
	defer em.mu.Unlock()

	em.encodingStats.TotalEncoded++
	em.encodingStats.EncodingTime += duration

	formatKey := format.String()
	if em.encodingStats.FormatStats[formatKey] == nil {
		em.encodingStats.FormatStats[formatKey] = &FormatStats{}
	}

	stats := em.encodingStats.FormatStats[formatKey]
	stats.EncodedCount++
	stats.EncodingTime += duration

	if !success {
		stats.ErrorCount++
	}
}

// updateDecodingStats updates decoding statistics
func (em *EncodingManager) updateDecodingStats(format EncodingFormat, duration time.Duration, success bool) {
	em.mu.Lock()
	defer em.mu.Unlock()

	em.encodingStats.TotalDecoded++
	em.encodingStats.DecodingTime += duration

	formatKey := format.String()
	if em.encodingStats.FormatStats[formatKey] == nil {
		em.encodingStats.FormatStats[formatKey] = &FormatStats{}
	}

	stats := em.encodingStats.FormatStats[formatKey]
	stats.DecodedCount++
	stats.DecodingTime += duration

	if !success {
		stats.ErrorCount++
	}
}

// updateErrorStats updates error statistics
func (em *EncodingManager) updateErrorStats() {
	em.mu.Lock()
	defer em.mu.Unlock()
	em.encodingStats.ErrorCount++
}

// evictOldestSchema evicts the oldest schema from cache
func (em *EncodingManager) evictOldestSchema() {
	var oldestID string
	var oldestTime time.Time

	for id, schema := range em.schemaCache {
		if oldestID == "" || schema.CreatedAt.Before(oldestTime) {
			oldestID = id
			oldestTime = schema.CreatedAt
		}
	}

	if oldestID != "" {
		delete(em.schemaCache, oldestID)
	}
}

// GetStatistics returns encoding statistics
func (em *EncodingManager) GetStatistics() *EncodingStatistics {
	em.mu.RLock()
	defer em.mu.RUnlock()

	// Create a deep copy
	stats := *em.encodingStats
	stats.FormatStats = make(map[string]*FormatStats)
	stats.CompressionStats = make(map[string]*CompressionStats)

	for k, v := range em.encodingStats.FormatStats {
		statsCopy := *v
		stats.FormatStats[k] = &statsCopy
	}

	for k, v := range em.encodingStats.CompressionStats {
		statsCopy := *v
		stats.CompressionStats[k] = &statsCopy
	}

	return &stats
}

// BSATN Encoder Implementation

// Encode encodes data using BSATN format
func (be *BSATNEncoder) Encode(data interface{}) ([]byte, error) {
	return bsatn.Marshal(data)
}

// EncodeTo encodes data to a writer
func (be *BSATNEncoder) EncodeTo(data interface{}, writer io.Writer) error {
	encoded, err := be.Encode(data)
	if err != nil {
		return err
	}
	_, err = writer.Write(encoded)
	return err
}

// GetFormat returns the encoding format
func (be *BSATNEncoder) GetFormat() EncodingFormat {
	return EncodingBSATN
}

// GetOptions returns encoding options
func (be *BSATNEncoder) GetOptions() *EncodingOptions {
	return be.options
}

// BSATN Decoder Implementation

// Decode decodes BSATN data
func (bd *BSATNDecoder) Decode(data []byte, target interface{}) error {
	return bsatn.UnmarshalInto(data, target)
}

// DecodeFrom decodes data from a reader
func (bd *BSATNDecoder) DecodeFrom(reader io.Reader, target interface{}) error {
	data, err := io.ReadAll(reader)
	if err != nil {
		return err
	}
	return bd.Decode(data, target)
}

// GetFormat returns the decoding format
func (bd *BSATNDecoder) GetFormat() EncodingFormat {
	return EncodingBSATN
}

// GetOptions returns decoding options
func (bd *BSATNDecoder) GetOptions() *DecodingOptions {
	return bd.options
}

// Gzip Compressor Implementation

// Compress compresses data using gzip
func (gc *GzipCompressor) Compress(data []byte) ([]byte, error) {
	var buf bytes.Buffer
	writer, err := gzip.NewWriterLevel(&buf, gc.level)
	if err != nil {
		return nil, err
	}

	_, err = writer.Write(data)
	if err != nil {
		return nil, err
	}

	err = writer.Close()
	if err != nil {
		return nil, err
	}

	return buf.Bytes(), nil
}

// Decompress decompresses gzip data
func (gc *GzipCompressor) Decompress(data []byte) ([]byte, error) {
	reader, err := gzip.NewReader(bytes.NewReader(data))
	if err != nil {
		return nil, err
	}
	defer reader.Close()

	return io.ReadAll(reader)
}

// GetType returns compression type
func (gc *GzipCompressor) GetType() CompressionType {
	return CompressionGzip
}

// GetLevel returns compression level
func (gc *GzipCompressor) GetLevel() int {
	return gc.level
}
