package protocol_test

import (
	"bytes"
	"compress/gzip"
	"encoding/binary"
	"testing"

	"github.com/andybalholm/brotli"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/client/protocol"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// --- ClientMessage encoding tests ---

func TestCallReducer_WriteBsatn(t *testing.T) {
	msg := &protocol.CallReducer{
		RequestID: 42,
		Flags:     0,
		Reducer:   "add_user",
		Args:      []byte{0xDE, 0xAD},
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	// Build expected bytes manually:
	// tag=3 (u8), request_id=42 (u32 LE), flags=0 (u8),
	// reducer="add_user" (u32 LE len + UTF-8), args (u32 LE len + raw bytes)
	exp := bsatn.NewWriter(64)
	exp.PutSumTag(3)
	exp.PutU32(42)
	exp.PutU8(0)
	exp.PutString("add_user")
	bsatn.WriteByteArray(exp, []byte{0xDE, 0xAD})
	expected := exp.Bytes()

	assert.Equal(t, expected, got)

	// Verify individual field positions
	assert.Equal(t, uint8(3), got[0], "sum tag should be 3 for CallReducer")
	assert.Equal(t, uint32(42), binary.LittleEndian.Uint32(got[1:5]), "request_id should be 42")
	assert.Equal(t, uint8(0), got[5], "flags should be 0")
}

func TestCallReducer_EmptyArgs(t *testing.T) {
	msg := &protocol.CallReducer{
		RequestID: 1,
		Flags:     0,
		Reducer:   "noop",
		Args:      nil,
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(3)
	exp.PutU32(1)
	exp.PutU8(0)
	exp.PutString("noop")
	bsatn.WriteByteArray(exp, nil)

	assert.Equal(t, exp.Bytes(), got)
}

func TestSubscribe_WriteBsatn(t *testing.T) {
	msg := &protocol.Subscribe{
		RequestID:    10,
		QuerySetID:   5,
		QueryStrings: []string{"SELECT * FROM users", "SELECT * FROM items"},
	}

	w := bsatn.NewWriter(128)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(128)
	exp.PutSumTag(0) // Subscribe tag
	exp.PutU32(10)   // request_id
	exp.PutU32(5)    // query_set_id
	exp.PutArrayLen(2)
	exp.PutString("SELECT * FROM users")
	exp.PutString("SELECT * FROM items")

	assert.Equal(t, exp.Bytes(), got)
	assert.Equal(t, uint8(0), got[0], "sum tag should be 0 for Subscribe")
}

func TestSubscribe_EmptyQueries(t *testing.T) {
	msg := &protocol.Subscribe{
		RequestID:    1,
		QuerySetID:   1,
		QueryStrings: nil,
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(0)
	exp.PutU32(1)
	exp.PutU32(1)
	exp.PutArrayLen(0)

	assert.Equal(t, exp.Bytes(), got)
}

func TestUnsubscribe_WriteBsatn(t *testing.T) {
	msg := &protocol.Unsubscribe{
		RequestID:  7,
		QuerySetID: 3,
		Flags:      protocol.UnsubscribeFlagsDefault,
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(1) // Unsubscribe tag
	exp.PutU32(7)    // request_id
	exp.PutU32(3)    // query_set_id
	exp.PutSumTag(0) // flags = Default (tag 0)

	assert.Equal(t, exp.Bytes(), got)
}

func TestUnsubscribe_SendDroppedRows(t *testing.T) {
	msg := &protocol.Unsubscribe{
		RequestID:  9,
		QuerySetID: 4,
		Flags:      protocol.UnsubscribeFlagsSendDroppedRows,
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(1) // Unsubscribe tag
	exp.PutU32(9)
	exp.PutU32(4)
	exp.PutSumTag(1) // flags = SendDroppedRows (tag 1)

	assert.Equal(t, exp.Bytes(), got)
}

func TestOneOffQuery_WriteBsatn(t *testing.T) {
	msg := &protocol.OneOffQuery{
		RequestID:   99,
		QueryString: "SELECT count(*) FROM users",
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(2) // OneOffQuery tag
	exp.PutU32(99)
	exp.PutString("SELECT count(*) FROM users")

	assert.Equal(t, exp.Bytes(), got)
}

func TestCallProcedure_WriteBsatn(t *testing.T) {
	msg := &protocol.CallProcedure{
		RequestID: 55,
		Flags:     1,
		Procedure: "my_proc",
		Args:      []byte{0x01, 0x02, 0x03},
	}

	w := bsatn.NewWriter(64)
	msg.WriteBsatn(w)
	got := w.Bytes()

	exp := bsatn.NewWriter(64)
	exp.PutSumTag(4) // CallProcedure tag
	exp.PutU32(55)
	exp.PutU8(1)
	exp.PutString("my_proc")
	bsatn.WriteByteArray(exp, []byte{0x01, 0x02, 0x03})

	assert.Equal(t, exp.Bytes(), got)
}

// --- Compression tests ---

func TestDecompressMessage_None(t *testing.T) {
	payload := []byte{0x01, 0x02, 0x03, 0x04}
	// Prepend compression tag 0 (none)
	data := append([]byte{0x00}, payload...)

	result, err := protocol.DecompressMessage(data)
	require.NoError(t, err)
	assert.Equal(t, payload, result)
}

func TestDecompressMessage_Gzip(t *testing.T) {
	original := []byte("hello world from spacetimedb")

	// Gzip compress the payload
	var buf bytes.Buffer
	gw := gzip.NewWriter(&buf)
	_, err := gw.Write(original)
	require.NoError(t, err)
	require.NoError(t, gw.Close())

	// Prepend compression tag 2 (gzip)
	data := append([]byte{0x02}, buf.Bytes()...)

	result, err := protocol.DecompressMessage(data)
	require.NoError(t, err)
	assert.Equal(t, original, result)
}

func TestDecompressMessage_EmptyPayload(t *testing.T) {
	_, err := protocol.DecompressMessage(nil)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "empty message")
}

func TestDecompressMessage_UnknownTag(t *testing.T) {
	data := []byte{0xFF, 0x01, 0x02}
	_, err := protocol.DecompressMessage(data)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "unknown compression tag")
}

// --- BsatnRowList decoding tests ---

func TestBsatnRowList_FixedSize(t *testing.T) {
	// Build BSATN for a BsatnRowList with FixedSizeHint(rowSize=4) and 8 bytes of row data (2 rows)
	w := bsatn.NewWriter(64)
	w.PutSumTag(0)  // FixedSize tag
	w.PutU16(4)     // row size = 4
	w.PutArrayLen(8) // 8 bytes of row data
	w.PutBytes([]byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08})

	r := bsatn.NewReader(w.Bytes())
	rl, err := protocol.ReadBsatnRowList(r)
	require.NoError(t, err)

	hint, ok := rl.SizeHint.(protocol.FixedSizeHint)
	require.True(t, ok, "expected FixedSizeHint")
	assert.Equal(t, uint16(4), hint.RowSize)
	assert.Equal(t, []byte{0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08}, rl.RowsData)

	// Test Rows()
	rows := rl.Rows()
	require.Len(t, rows, 2)
	assert.Equal(t, []byte{0x01, 0x02, 0x03, 0x04}, rows[0])
	assert.Equal(t, []byte{0x05, 0x06, 0x07, 0x08}, rows[1])

	// Test Len()
	assert.Equal(t, 2, rl.Len())
}

func TestBsatnRowList_RowOffsets(t *testing.T) {
	// Build BSATN for a BsatnRowList with RowOffsetsHint and variable-size rows
	w := bsatn.NewWriter(64)
	w.PutSumTag(1)    // RowOffsets tag
	w.PutArrayLen(3)  // 3 offsets
	w.PutU64(0)       // row 0 starts at 0
	w.PutU64(2)       // row 1 starts at 2
	w.PutU64(5)       // row 2 starts at 5
	rowData := []byte{0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11}
	w.PutArrayLen(uint32(len(rowData)))
	w.PutBytes(rowData)

	r := bsatn.NewReader(w.Bytes())
	rl, err := protocol.ReadBsatnRowList(r)
	require.NoError(t, err)

	hint, ok := rl.SizeHint.(protocol.RowOffsetsHint)
	require.True(t, ok, "expected RowOffsetsHint")
	assert.Equal(t, []uint64{0, 2, 5}, hint.Offsets)

	rows := rl.Rows()
	require.Len(t, rows, 3)
	assert.Equal(t, []byte{0xAA, 0xBB}, rows[0])
	assert.Equal(t, []byte{0xCC, 0xDD, 0xEE}, rows[1])
	assert.Equal(t, []byte{0xFF, 0x11}, rows[2])

	assert.Equal(t, 3, rl.Len())
}

func TestBsatnRowList_EmptyRowsData(t *testing.T) {
	w := bsatn.NewWriter(32)
	w.PutSumTag(0)    // FixedSize
	w.PutU16(4)       // row size
	w.PutArrayLen(0)  // 0 bytes of data

	r := bsatn.NewReader(w.Bytes())
	rl, err := protocol.ReadBsatnRowList(r)
	require.NoError(t, err)
	assert.Equal(t, 0, rl.Len())
	assert.Nil(t, rl.Rows())
}

func TestBsatnRowList_NilRowList(t *testing.T) {
	var rl *protocol.BsatnRowList
	assert.Equal(t, 0, rl.Len())
	assert.Nil(t, rl.Rows())
}

func TestBsatnRowList_ZeroRowSize(t *testing.T) {
	w := bsatn.NewWriter(32)
	w.PutSumTag(0)    // FixedSize
	w.PutU16(0)       // row size = 0
	w.PutArrayLen(4)  // 4 bytes of data
	w.PutBytes([]byte{0x01, 0x02, 0x03, 0x04})

	r := bsatn.NewReader(w.Bytes())
	rl, err := protocol.ReadBsatnRowList(r)
	require.NoError(t, err)
	assert.Equal(t, 0, rl.Len())
	assert.Nil(t, rl.Rows())
}

func TestBsatnRowList_InvalidTag(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutSumTag(99) // invalid tag

	r := bsatn.NewReader(w.Bytes())
	_, err := protocol.ReadBsatnRowList(r)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "invalid tag 99")
}

// --- ServerMessage decoding tests ---

func TestReadServerMessage_InitialConnection(t *testing.T) {
	// Build BSATN for an InitialConnection message
	w := bsatn.NewWriter(128)
	w.PutSumTag(0) // InitialConnection tag

	// Identity: 32 bytes
	var identityBytes [32]byte
	for i := range identityBytes {
		identityBytes[i] = byte(i)
	}
	w.PutBytes(identityBytes[:])

	// ConnectionId: 16 bytes
	var connIdBytes [16]byte
	for i := range connIdBytes {
		connIdBytes[i] = byte(0xA0 + i)
	}
	w.PutBytes(connIdBytes[:])

	// Token: string
	w.PutString("my-auth-token-123")

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	ic, ok := msg.(*protocol.InitialConnection)
	require.True(t, ok, "expected *InitialConnection")
	assert.Equal(t, identityBytes, ic.Identity.Bytes())
	assert.Equal(t, connIdBytes, ic.ConnectionID.Bytes())
	assert.Equal(t, "my-auth-token-123", ic.Token)
}

func TestReadServerMessage_SubscribeApplied(t *testing.T) {
	w := bsatn.NewWriter(128)
	w.PutSumTag(1)  // SubscribeApplied tag
	w.PutU32(42)    // request_id
	w.PutU32(7)     // query_set_id

	// QueryRows: tables (Vec<SingleTableRows>) - empty
	w.PutArrayLen(0)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	sa, ok := msg.(*protocol.SubscribeApplied)
	require.True(t, ok, "expected *SubscribeApplied")
	assert.Equal(t, uint32(42), sa.RequestID)
	assert.Equal(t, uint32(7), sa.QuerySetID)
	assert.Empty(t, sa.Rows.Tables)
}

func TestReadServerMessage_SubscribeApplied_WithRows(t *testing.T) {
	w := bsatn.NewWriter(256)
	w.PutSumTag(1)  // SubscribeApplied tag
	w.PutU32(1)     // request_id
	w.PutU32(2)     // query_set_id

	// QueryRows: tables (Vec<SingleTableRows>) - 1 table
	w.PutArrayLen(1)
	// SingleTableRows: table_name (string) + rows (BsatnRowList)
	w.PutString("users")
	// BsatnRowList: FixedSize(8), 16 bytes of data (2 rows)
	w.PutSumTag(0)   // FixedSize
	w.PutU16(8)      // row size
	w.PutArrayLen(16) // 16 bytes
	rowData := make([]byte, 16)
	for i := range rowData {
		rowData[i] = byte(i + 1)
	}
	w.PutBytes(rowData)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	sa, ok := msg.(*protocol.SubscribeApplied)
	require.True(t, ok, "expected *SubscribeApplied")
	require.Len(t, sa.Rows.Tables, 1)
	assert.Equal(t, "users", sa.Rows.Tables[0].TableName)
	assert.Equal(t, 2, sa.Rows.Tables[0].Rows.Len())
}

func TestReadServerMessage_UnsubscribeApplied_NoRows(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(2)  // UnsubscribeApplied tag
	w.PutU32(10)    // request_id
	w.PutU32(3)     // query_set_id
	w.PutSumTag(1)  // Option::None for rows

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	ua, ok := msg.(*protocol.UnsubscribeApplied)
	require.True(t, ok, "expected *UnsubscribeApplied")
	assert.Equal(t, uint32(10), ua.RequestID)
	assert.Equal(t, uint32(3), ua.QuerySetID)
	assert.Nil(t, ua.Rows)
}

func TestReadServerMessage_UnsubscribeApplied_WithRows(t *testing.T) {
	w := bsatn.NewWriter(128)
	w.PutSumTag(2)  // UnsubscribeApplied tag
	w.PutU32(10)    // request_id
	w.PutU32(3)     // query_set_id
	w.PutSumTag(0)  // Option::Some for rows
	// QueryRows: empty tables
	w.PutArrayLen(0)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	ua, ok := msg.(*protocol.UnsubscribeApplied)
	require.True(t, ok, "expected *UnsubscribeApplied")
	require.NotNil(t, ua.Rows)
	assert.Empty(t, ua.Rows.Tables)
}

func TestReadServerMessage_SubscriptionError(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(3)  // SubscriptionError tag
	w.PutSumTag(0)  // Option::Some for request_id
	w.PutU32(77)    // request_id value
	w.PutU32(5)     // query_set_id
	w.PutString("table not found")

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	se, ok := msg.(*protocol.SubscriptionError)
	require.True(t, ok, "expected *SubscriptionError")
	require.NotNil(t, se.RequestID)
	assert.Equal(t, uint32(77), *se.RequestID)
	assert.Equal(t, uint32(5), se.QuerySetID)
	assert.Equal(t, "table not found", se.Error)
}

func TestReadServerMessage_SubscriptionError_NoRequestID(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(3)  // SubscriptionError tag
	w.PutSumTag(1)  // Option::None for request_id
	w.PutU32(5)     // query_set_id
	w.PutString("unknown error")

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	se, ok := msg.(*protocol.SubscriptionError)
	require.True(t, ok, "expected *SubscriptionError")
	assert.Nil(t, se.RequestID)
	assert.Equal(t, "unknown error", se.Error)
}

func TestReadServerMessage_TransactionUpdate_Empty(t *testing.T) {
	w := bsatn.NewWriter(32)
	w.PutSumTag(4)    // TransactionUpdate tag
	w.PutArrayLen(0)  // empty query_sets

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	tu, ok := msg.(*protocol.TransactionUpdate)
	require.True(t, ok, "expected *TransactionUpdate")
	assert.Empty(t, tu.QuerySets)
}

func TestReadServerMessage_TransactionUpdate_WithPersistentRows(t *testing.T) {
	w := bsatn.NewWriter(256)
	w.PutSumTag(4)    // TransactionUpdate tag
	w.PutArrayLen(1)  // 1 query set update

	// QuerySetUpdate: query_set_id + tables
	w.PutU32(1) // query_set_id
	w.PutArrayLen(1) // 1 table update

	// TableUpdate: table_name + rows
	w.PutString("players")
	w.PutArrayLen(1) // 1 TableUpdateRows

	// PersistentTableRows: tag=0, inserts + deletes
	w.PutSumTag(0) // PersistentTable tag
	// inserts: BsatnRowList (FixedSize, 4 bytes = 1 row of size 4)
	w.PutSumTag(0)   // FixedSize
	w.PutU16(4)      // row size
	w.PutArrayLen(4) // 4 bytes
	w.PutBytes([]byte{0x0A, 0x0B, 0x0C, 0x0D})
	// deletes: BsatnRowList (FixedSize, 0 bytes = 0 rows)
	w.PutSumTag(0)   // FixedSize
	w.PutU16(4)      // row size
	w.PutArrayLen(0) // 0 bytes

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	tu, ok := msg.(*protocol.TransactionUpdate)
	require.True(t, ok, "expected *TransactionUpdate")
	require.Len(t, tu.QuerySets, 1)
	assert.Equal(t, uint32(1), tu.QuerySets[0].QuerySetID)
	require.Len(t, tu.QuerySets[0].Tables, 1)
	assert.Equal(t, "players", tu.QuerySets[0].Tables[0].TableName)
	require.Len(t, tu.QuerySets[0].Tables[0].Rows, 1)

	ptr, ok := tu.QuerySets[0].Tables[0].Rows[0].(*protocol.PersistentTableRows)
	require.True(t, ok, "expected *PersistentTableRows")
	assert.Equal(t, 1, ptr.Inserts.Len())
	assert.Equal(t, 0, ptr.Deletes.Len())
}

func TestReadServerMessage_OneOffQueryResult_Ok(t *testing.T) {
	w := bsatn.NewWriter(128)
	w.PutSumTag(5)  // OneOffQueryResult tag
	w.PutU32(33)    // request_id
	w.PutSumTag(0)  // Result::Ok tag

	// QueryRows: 1 table, 1 row
	w.PutArrayLen(1)
	w.PutString("counts")
	w.PutSumTag(0)   // FixedSize
	w.PutU16(4)      // row size
	w.PutArrayLen(4) // 4 bytes
	w.PutBytes([]byte{0x01, 0x00, 0x00, 0x00}) // row data

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	oqr, ok := msg.(*protocol.OneOffQueryResult)
	require.True(t, ok, "expected *OneOffQueryResult")
	assert.Equal(t, uint32(33), oqr.RequestID)
	require.NotNil(t, oqr.ResultOk)
	assert.Empty(t, oqr.ResultErr)
	require.Len(t, oqr.ResultOk.Tables, 1)
}

func TestReadServerMessage_OneOffQueryResult_Err(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(5)  // OneOffQueryResult tag
	w.PutU32(33)    // request_id
	w.PutSumTag(1)  // Result::Err tag
	w.PutString("syntax error near SELECT")

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	oqr, ok := msg.(*protocol.OneOffQueryResult)
	require.True(t, ok, "expected *OneOffQueryResult")
	assert.Equal(t, uint32(33), oqr.RequestID)
	assert.Nil(t, oqr.ResultOk)
	assert.Equal(t, "syntax error near SELECT", oqr.ResultErr)
}

func TestReadServerMessage_ReducerResult_OkEmpty(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(6) // ReducerResult tag
	w.PutU32(100)  // request_id
	w.PutI64(1234567890) // timestamp (microseconds)
	w.PutSumTag(1)       // ReducerOutcome::OkEmpty tag

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	rr, ok := msg.(*protocol.ReducerResult)
	require.True(t, ok, "expected *ReducerResult")
	assert.Equal(t, uint32(100), rr.RequestID)
	assert.Equal(t, int64(1234567890), rr.Timestamp.Microseconds())

	_, ok = rr.Result.(*protocol.ReducerOkEmpty)
	assert.True(t, ok, "expected *ReducerOkEmpty outcome")
}

func TestReadServerMessage_ReducerResult_Ok(t *testing.T) {
	w := bsatn.NewWriter(128)
	w.PutSumTag(6) // ReducerResult tag
	w.PutU32(200)  // request_id
	w.PutI64(9999) // timestamp
	w.PutSumTag(0) // ReducerOutcome::Ok tag

	// ReducerOk: ret_value (byte array) + transaction_update
	bsatn.WriteByteArray(w, []byte{0x42})
	// TransactionUpdate: empty query_sets
	w.PutArrayLen(0)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	rr, ok := msg.(*protocol.ReducerResult)
	require.True(t, ok, "expected *ReducerResult")

	rok, ok := rr.Result.(*protocol.ReducerOk)
	require.True(t, ok, "expected *ReducerOk outcome")
	assert.Equal(t, []byte{0x42}, rok.RetValue)
	require.NotNil(t, rok.TransactionUpdate)
	assert.Empty(t, rok.TransactionUpdate.QuerySets)
}

func TestReadServerMessage_ReducerResult_Err(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(6) // ReducerResult tag
	w.PutU32(300)  // request_id
	w.PutI64(5555) // timestamp
	w.PutSumTag(2) // ReducerOutcome::Err tag
	bsatn.WriteByteArray(w, []byte{0xEE, 0xFF})

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	rr, ok := msg.(*protocol.ReducerResult)
	require.True(t, ok)

	re, ok := rr.Result.(*protocol.ReducerErr)
	require.True(t, ok, "expected *ReducerErr outcome")
	assert.Equal(t, []byte{0xEE, 0xFF}, re.ErrorBytes)
}

func TestReadServerMessage_ReducerResult_InternalError(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(6) // ReducerResult tag
	w.PutU32(400)  // request_id
	w.PutI64(7777) // timestamp
	w.PutSumTag(3) // ReducerOutcome::InternalError tag
	w.PutString("reducer panicked")

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	rr, ok := msg.(*protocol.ReducerResult)
	require.True(t, ok)

	rie, ok := rr.Result.(*protocol.ReducerInternalError)
	require.True(t, ok, "expected *ReducerInternalError outcome")
	assert.Equal(t, "reducer panicked", rie.Message)
}

func TestReadServerMessage_ProcedureResult_Returned(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(7) // ProcedureResult tag

	// status: Returned(Bytes)
	w.PutSumTag(0)
	bsatn.WriteByteArray(w, []byte{0xAA, 0xBB})

	// timestamp
	w.PutI64(111222333)

	// total_host_execution_duration
	w.PutI64(5000)

	// request_id
	w.PutU32(50)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	pr, ok := msg.(*protocol.ProcedureResult)
	require.True(t, ok, "expected *ProcedureResult")
	assert.Equal(t, uint32(50), pr.RequestID)
	assert.Equal(t, int64(111222333), pr.Timestamp.Microseconds())
	assert.Equal(t, int64(5000), pr.TotalHostExecutionDuration.Microseconds())

	ret, ok := pr.Status.(*protocol.ProcedureReturned)
	require.True(t, ok, "expected *ProcedureReturned")
	assert.Equal(t, []byte{0xAA, 0xBB}, ret.Value)
}

func TestReadServerMessage_ProcedureResult_InternalError(t *testing.T) {
	w := bsatn.NewWriter(64)
	w.PutSumTag(7) // ProcedureResult tag

	// status: InternalError(String)
	w.PutSumTag(1)
	w.PutString("host error")

	// timestamp
	w.PutI64(0)

	// total_host_execution_duration
	w.PutI64(0)

	// request_id
	w.PutU32(51)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	pr, ok := msg.(*protocol.ProcedureResult)
	require.True(t, ok, "expected *ProcedureResult")

	pie, ok := pr.Status.(*protocol.ProcedureInternalError)
	require.True(t, ok, "expected *ProcedureInternalError")
	assert.Equal(t, "host error", pie.Message)
}

func TestReadServerMessage_InvalidTag(t *testing.T) {
	w := bsatn.NewWriter(8)
	w.PutSumTag(99) // invalid ServerMessage tag

	r := bsatn.NewReader(w.Bytes())
	_, err := protocol.ReadServerMessage(r)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "invalid tag 99")
}

func TestReadServerMessage_EventTableRows(t *testing.T) {
	w := bsatn.NewWriter(128)
	w.PutSumTag(4)    // TransactionUpdate tag
	w.PutArrayLen(1)  // 1 query set update

	w.PutU32(2) // query_set_id
	w.PutArrayLen(1) // 1 table update

	w.PutString("events")
	w.PutArrayLen(1) // 1 TableUpdateRows

	// EventTableRows: tag=1, events
	w.PutSumTag(1) // EventTable tag
	w.PutSumTag(0)   // FixedSize
	w.PutU16(2)      // row size
	w.PutArrayLen(4) // 4 bytes = 2 rows
	w.PutBytes([]byte{0x01, 0x02, 0x03, 0x04})

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	tu, ok := msg.(*protocol.TransactionUpdate)
	require.True(t, ok)
	require.Len(t, tu.QuerySets, 1)
	require.Len(t, tu.QuerySets[0].Tables, 1)
	require.Len(t, tu.QuerySets[0].Tables[0].Rows, 1)

	etr, ok := tu.QuerySets[0].Tables[0].Rows[0].(*protocol.EventTableRows)
	require.True(t, ok, "expected *EventTableRows")
	assert.Equal(t, 2, etr.Events.Len())
}

// --- Brotli compression test ---

func TestDecompressMessage_Brotli(t *testing.T) {
	original := []byte("brotli compressed data for spacetimedb protocol")

	// Brotli compress the payload
	var buf bytes.Buffer
	bw := brotli.NewWriterLevel(&buf, brotli.DefaultCompression)
	_, err := bw.Write(original)
	require.NoError(t, err)
	require.NoError(t, bw.Close())

	// Prepend compression tag 1 (brotli)
	data := append([]byte{0x01}, buf.Bytes()...)

	result, err := protocol.DecompressMessage(data)
	require.NoError(t, err)
	assert.Equal(t, original, result)
}

func TestDecompressMessage_None_SingleByte(t *testing.T) {
	// Tag byte only, no payload
	data := []byte{0x00}
	result, err := protocol.DecompressMessage(data)
	require.NoError(t, err)
	assert.Empty(t, result)
}

func TestDecompressMessage_None_EmptyAfterTag(t *testing.T) {
	data := []byte{0x00}
	result, err := protocol.DecompressMessage(data)
	require.NoError(t, err)
	assert.Equal(t, []byte{}, result)
}

// --- Edge case: large messages ---

func TestSubscribe_ManyQueries(t *testing.T) {
	queries := make([]string, 100)
	for i := range queries {
		queries[i] = "SELECT * FROM table_" + string(rune('A'+i%26))
	}

	msg := &protocol.Subscribe{
		RequestID:    1,
		QuerySetID:   1,
		QueryStrings: queries,
	}

	w := bsatn.NewWriter(4096)
	msg.WriteBsatn(w)
	got := w.Bytes()

	require.NotEmpty(t, got)
	assert.Equal(t, uint8(0), got[0], "tag should be 0")

	// Verify array length is encoded as 100
	arrayLenOffset := 1 + 4 + 4 // tag + request_id + query_set_id
	assert.Equal(t, uint32(100), binary.LittleEndian.Uint32(got[arrayLenOffset:arrayLenOffset+4]))
}

func TestCallReducer_LargeArgs(t *testing.T) {
	largeArgs := make([]byte, 65536)
	for i := range largeArgs {
		largeArgs[i] = byte(i % 256)
	}

	msg := &protocol.CallReducer{
		RequestID: 1,
		Flags:     0,
		Reducer:   "bulk_insert",
		Args:      largeArgs,
	}

	w := bsatn.NewWriter(70000)
	msg.WriteBsatn(w)
	got := w.Bytes()
	require.NotEmpty(t, got)

	// Verify the encoded size is plausible
	// tag(1) + request_id(4) + flags(1) + string_len(4) + "bulk_insert"(11) + array_len(4) + 65536
	expectedMinSize := 1 + 4 + 1 + 4 + 11 + 4 + 65536
	assert.GreaterOrEqual(t, len(got), expectedMinSize)
}

func TestBsatnRowList_ManyRowsFixedSize(t *testing.T) {
	rowSize := uint16(8)
	numRows := 1000
	rowData := make([]byte, int(rowSize)*numRows)
	for i := range rowData {
		rowData[i] = byte(i % 256)
	}

	w := bsatn.NewWriter(len(rowData) + 16)
	w.PutSumTag(0)
	w.PutU16(rowSize)
	w.PutArrayLen(uint32(len(rowData)))
	w.PutBytes(rowData)

	r := bsatn.NewReader(w.Bytes())
	rl, err := protocol.ReadBsatnRowList(r)
	require.NoError(t, err)
	assert.Equal(t, numRows, rl.Len())

	rows := rl.Rows()
	require.Len(t, rows, numRows)
	for i, row := range rows {
		require.Len(t, row, int(rowSize), "row %d should have size %d", i, rowSize)
	}
}

// --- ServerMessage decoding: TransactionUpdate with multiple query sets and tables ---

func TestReadServerMessage_TransactionUpdate_MultipleQuerySets(t *testing.T) {
	w := bsatn.NewWriter(512)
	w.PutSumTag(4)    // TransactionUpdate tag
	w.PutArrayLen(2)  // 2 query set updates

	// QuerySetUpdate 1
	w.PutU32(10) // query_set_id
	w.PutArrayLen(1) // 1 table
	w.PutString("users")
	w.PutArrayLen(1) // 1 TableUpdateRows
	w.PutSumTag(0) // PersistentTable
	// inserts
	w.PutSumTag(0); w.PutU16(4); w.PutArrayLen(4)
	w.PutBytes([]byte{0x01, 0x02, 0x03, 0x04})
	// deletes
	w.PutSumTag(0); w.PutU16(4); w.PutArrayLen(0)

	// QuerySetUpdate 2
	w.PutU32(20) // query_set_id
	w.PutArrayLen(1) // 1 table
	w.PutString("items")
	w.PutArrayLen(1) // 1 TableUpdateRows
	w.PutSumTag(0) // PersistentTable
	// inserts
	w.PutSumTag(0); w.PutU16(2); w.PutArrayLen(4)
	w.PutBytes([]byte{0xAA, 0xBB, 0xCC, 0xDD})
	// deletes
	w.PutSumTag(0); w.PutU16(2); w.PutArrayLen(0)

	r := bsatn.NewReader(w.Bytes())
	msg, err := protocol.ReadServerMessage(r)
	require.NoError(t, err)

	tu, ok := msg.(*protocol.TransactionUpdate)
	require.True(t, ok)
	require.Len(t, tu.QuerySets, 2)
	assert.Equal(t, uint32(10), tu.QuerySets[0].QuerySetID)
	assert.Equal(t, "users", tu.QuerySets[0].Tables[0].TableName)
	assert.Equal(t, uint32(20), tu.QuerySets[1].QuerySetID)
	assert.Equal(t, "items", tu.QuerySets[1].Tables[0].TableName)
}

// --- BSATN encode helper round-trip ---

func TestBsatnEncode_CallReducer(t *testing.T) {
	msg := &protocol.CallReducer{
		RequestID: 1,
		Flags:     0,
		Reducer:   "test",
		Args:      []byte{0x01},
	}

	encoded := bsatn.Encode(msg)
	require.NotEmpty(t, encoded)

	// Verify the tag byte
	assert.Equal(t, uint8(3), encoded[0])
}

func TestBsatnEncode_AllClientMessages(t *testing.T) {
	messages := []protocol.ClientMessage{
		&protocol.Subscribe{RequestID: 1, QuerySetID: 1, QueryStrings: []string{"SELECT 1"}},
		&protocol.Unsubscribe{RequestID: 2, QuerySetID: 1, Flags: protocol.UnsubscribeFlagsDefault},
		&protocol.OneOffQuery{RequestID: 3, QueryString: "SELECT 1"},
		&protocol.CallReducer{RequestID: 4, Flags: 0, Reducer: "test", Args: nil},
		&protocol.CallProcedure{RequestID: 5, Flags: 0, Procedure: "test", Args: nil},
	}

	expectedTags := []uint8{0, 1, 2, 3, 4}

	for i, msg := range messages {
		encoded := bsatn.Encode(msg)
		require.NotEmpty(t, encoded, "message %d should encode", i)
		assert.Equal(t, expectedTags[i], encoded[0], "message %d should have tag %d", i, expectedTags[i])
	}
}
