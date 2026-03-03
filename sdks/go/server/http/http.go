package http

import (
	"fmt"

	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/server/sys"
)

// Method represents an HTTP method as a BSATN sum type.
type method uint8

const (
	MethodGet       method = 0
	MethodHead      method = 1
	MethodPost      method = 2
	MethodPut       method = 3
	MethodDelete    method = 4
	MethodConnect   method = 5
	MethodOptions   method = 6
	MethodTrace     method = 7
	MethodPatch     method = 8
	MethodExtension method = 9 // carries a string payload
)

// version represents an HTTP version as a BSATN sum type.
type version uint8

const (
	versionHTTP09 version = 0
	versionHTTP10 version = 1
	versionHTTP11 version = 2
	versionHTTP2  version = 3
	versionHTTP3  version = 4
)

// writeMethod writes a Method sum type to BSATN.
// Simple variants (0-8) are tag-only (empty product payload).
// Extension(9) carries a String payload.
func writeMethod(w bsatn.Writer, m method) {
	w.PutSumTag(uint8(m))
	// Tags 0-8 have no payload (empty product). Tag 9 has a string payload.
	// We only support standard methods in this implementation.
}

// writeVersion writes a Version sum type to BSATN (tag-only, empty product payload).
func writeVersion(w bsatn.Writer, v version) {
	w.PutSumTag(uint8(v))
}

// writeHeaders writes a Headers product type to BSATN.
// Headers is a product with a single field: entries (array of HttpHeaderPair).
func writeHeaders(w bsatn.Writer, headers map[string]string) {
	w.PutArrayLen(uint32(len(headers)))
	for name, value := range headers {
		// HttpHeaderPair is a product: name (String), value (byte array)
		w.PutString(name)
		// value is Box<[u8]> which encodes as BSATN array of u8: u32 len + raw bytes
		w.PutArrayLen(uint32(len(value)))
		w.PutBytes([]byte(value))
	}
}

// writeOptionNone writes an Option::None (tag 1, empty payload).
func writeOptionNone(w bsatn.Writer) {
	w.PutSumTag(1)
}

// encodeRequest BSATN-encodes an HttpRequest product type.
// Request fields (in order): method, headers, timeout (Option<TimeDuration>), uri, version.
func encodeRequest(m method, uri string, headers map[string]string) []byte {
	w := bsatn.NewWriter(256)

	// Field 1: method
	writeMethod(w, m)

	// Field 2: headers
	writeHeaders(w, headers)

	// Field 3: timeout (Option<TimeDuration>) - always None for now
	writeOptionNone(w)

	// Field 4: uri
	w.PutString(uri)

	// Field 5: version - default to HTTP/1.1
	writeVersion(w, versionHTTP11)

	return w.Bytes()
}

// response holds the decoded HTTP response.
type response struct {
	code uint16
}

// decodeResponse decodes a BSATN-encoded HttpResponse.
// Response fields: headers (Headers), version (Version), code (u16).
func decodeResponse(data []byte) (*response, error) {
	r := bsatn.NewReader(data)

	// Field 1: headers (product with 1 field: array of HttpHeaderPair)
	numHeaders, err := r.GetArrayLen()
	if err != nil {
		return nil, fmt.Errorf("decode response headers length: %w", err)
	}
	for i := uint32(0); i < numHeaders; i++ {
		// HttpHeaderPair: name (String), value (byte array)
		// Skip name
		nameLen, err := r.GetU32()
		if err != nil {
			return nil, fmt.Errorf("decode header name length: %w", err)
		}
		if _, err := r.GetBytes(int(nameLen)); err != nil {
			return nil, fmt.Errorf("decode header name: %w", err)
		}
		// Skip value (byte array: u32 len + bytes)
		valueLen, err := r.GetU32()
		if err != nil {
			return nil, fmt.Errorf("decode header value length: %w", err)
		}
		if _, err := r.GetBytes(int(valueLen)); err != nil {
			return nil, fmt.Errorf("decode header value: %w", err)
		}
	}

	// Field 2: version (sum type, tag only for standard versions)
	if _, err := r.GetSumTag(); err != nil {
		return nil, fmt.Errorf("decode response version: %w", err)
	}

	// Field 3: code (u16)
	code, err := r.GetU16()
	if err != nil {
		return nil, fmt.Errorf("decode response code: %w", err)
	}

	return &response{code: code}, nil
}

// decodeBsatnString decodes a BSATN-encoded string (u32 len + UTF-8 bytes).
func decodeBsatnString(data []byte) (string, error) {
	r := bsatn.NewReader(data)
	return r.GetString()
}

// Get performs an HTTP GET request and returns the status code and response body.
func Get(uri string) (uint16, []byte, error) {
	return Send(MethodGet, uri, nil, nil)
}

// Send performs an HTTP request with the given method, URI, headers, and body.
// Returns the HTTP status code and response body.
func Send(m method, uri string, headers map[string]string, body []byte) (uint16, []byte, error) {
	requestBsatn := encodeRequest(m, uri, headers)

	responseSrc, bodySrc, err := sys.ProcedureHttpRequest(requestBsatn, body)
	if err != nil {
		// On HTTP_ERROR, responseSrc has BSATN-encoded error string.
		if responseSrc != 0 {
			errData, readErr := sys.ReadBytesSource(responseSrc)
			if readErr == nil && len(errData) > 0 {
				errMsg, decErr := decodeBsatnString(errData)
				if decErr == nil {
					return 0, nil, fmt.Errorf("%s", errMsg)
				}
			}
		}
		return 0, nil, fmt.Errorf("http request failed: %w", err)
	}

	// Read response BSATN.
	respData, err := sys.ReadBytesSource(responseSrc)
	if err != nil {
		return 0, nil, fmt.Errorf("read response: %w", err)
	}

	resp, err := decodeResponse(respData)
	if err != nil {
		return 0, nil, fmt.Errorf("decode response: %w", err)
	}

	// Read response body.
	respBody, err := sys.ReadBytesSource(bodySrc)
	if err != nil {
		return 0, nil, fmt.Errorf("read response body: %w", err)
	}

	return resp.code, respBody, nil
}
