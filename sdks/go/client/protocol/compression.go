package protocol

import (
	"bytes"
	"compress/gzip"
	"fmt"
	"io"

	"github.com/andybalholm/brotli"
)

// Compression identifies the compression algorithm used for a server message.
type Compression uint8

const (
	// CompressionNone means no compression.
	CompressionNone Compression = 0
	// CompressionBrotli means brotli compression.
	CompressionBrotli Compression = 1
	// CompressionGzip means gzip compression.
	CompressionGzip Compression = 2
)

// DecompressMessage reads the leading compression tag byte and
// decompresses the remaining payload accordingly.
func DecompressMessage(data []byte) ([]byte, error) {
	if len(data) == 0 {
		return nil, fmt.Errorf("protocol: empty message")
	}

	tag := Compression(data[0])
	payload := data[1:]

	switch tag {
	case CompressionNone:
		return payload, nil
	case CompressionBrotli:
		r := brotli.NewReader(bytes.NewReader(payload))
		return io.ReadAll(r)
	case CompressionGzip:
		gr, err := gzip.NewReader(bytes.NewReader(payload))
		if err != nil {
			return nil, fmt.Errorf("protocol: gzip init: %w", err)
		}
		defer gr.Close()
		return io.ReadAll(gr)
	default:
		return nil, fmt.Errorf("protocol: unknown compression tag: %d", tag)
	}
}
