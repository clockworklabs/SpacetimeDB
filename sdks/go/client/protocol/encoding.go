package protocol

import (
	"github.com/clockworklabs/SpacetimeDB/sdks/go/bsatn"
	"github.com/clockworklabs/SpacetimeDB/sdks/go/types"
)

// readInitialConnection reads an InitialConnection from a BSATN reader.
// Fields: identity(32B), connection_id(16B), token(string).
func readInitialConnection(r bsatn.Reader) (*InitialConnection, error) {
	identity, err := types.ReadIdentity(r)
	if err != nil {
		return nil, err
	}

	connID, err := types.ReadConnectionId(r)
	if err != nil {
		return nil, err
	}

	token, err := r.GetString()
	if err != nil {
		return nil, err
	}

	return &InitialConnection{
		Identity:     identity,
		ConnectionID: connID,
		Token:        token,
	}, nil
}

// readSubscribeApplied reads a SubscribeApplied from a BSATN reader.
// Fields: request_id(u32), query_set_id(QuerySetId{u32}), rows(QueryRows).
func readSubscribeApplied(r bsatn.Reader) (*SubscribeApplied, error) {
	requestID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	querySetID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	rows, err := readQueryRows(r)
	if err != nil {
		return nil, err
	}

	return &SubscribeApplied{
		RequestID:  requestID,
		QuerySetID: querySetID,
		Rows:       *rows,
	}, nil
}

// readUnsubscribeApplied reads an UnsubscribeApplied from a BSATN reader.
// Fields: request_id(u32), query_set_id(QuerySetId{u32}), rows(Option<QueryRows>).
func readUnsubscribeApplied(r bsatn.Reader) (*UnsubscribeApplied, error) {
	requestID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	querySetID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	rows, err := bsatn.ReadOption(r, func(r bsatn.Reader) (QueryRows, error) {
		qr, err := readQueryRows(r)
		if err != nil {
			return QueryRows{}, err
		}
		return *qr, nil
	})
	if err != nil {
		return nil, err
	}

	return &UnsubscribeApplied{
		RequestID:  requestID,
		QuerySetID: querySetID,
		Rows:       rows,
	}, nil
}

// readSubscriptionError reads a SubscriptionError from a BSATN reader.
// Fields: request_id(Option<u32>), query_set_id(QuerySetId{u32}), error(string).
func readSubscriptionError(r bsatn.Reader) (*SubscriptionError, error) {
	requestID, err := bsatn.ReadOption(r, func(r bsatn.Reader) (uint32, error) {
		return r.GetU32()
	})
	if err != nil {
		return nil, err
	}

	querySetID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	errMsg, err := r.GetString()
	if err != nil {
		return nil, err
	}

	return &SubscriptionError{
		RequestID:  requestID,
		QuerySetID: querySetID,
		Error:      errMsg,
	}, nil
}

// readTransactionUpdate reads a TransactionUpdate from a BSATN reader.
// Fields: query_sets(Vec<QuerySetUpdate>).
func readTransactionUpdate(r bsatn.Reader) (*TransactionUpdate, error) {
	querySets, err := bsatn.ReadArray(r, readQuerySetUpdate)
	if err != nil {
		return nil, err
	}

	return &TransactionUpdate{
		QuerySets: querySets,
	}, nil
}

// readOneOffQueryResult reads a OneOffQueryResult from a BSATN reader.
// Fields: request_id(u32), result(Result<QueryRows, String>).
// Result is a sum type: tag 0 = Ok(QueryRows), tag 1 = Err(String).
func readOneOffQueryResult(r bsatn.Reader) (*OneOffQueryResult, error) {
	requestID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	resultTag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	msg := &OneOffQueryResult{RequestID: requestID}

	switch resultTag {
	case 0: // Ok(QueryRows)
		rows, err := readQueryRows(r)
		if err != nil {
			return nil, err
		}
		msg.ResultOk = rows
	case 1: // Err(String)
		errMsg, err := r.GetString()
		if err != nil {
			return nil, err
		}
		msg.ResultErr = errMsg
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: resultTag, SumName: "Result<QueryRows, String>"}
	}

	return msg, nil
}

// readReducerResult reads a ReducerResult from a BSATN reader.
// Fields: request_id(u32), timestamp(i64), result(ReducerOutcome).
func readReducerResult(r bsatn.Reader) (*ReducerResult, error) {
	requestID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	timestamp, err := types.ReadTimestamp(r)
	if err != nil {
		return nil, err
	}

	outcome, err := readReducerOutcome(r)
	if err != nil {
		return nil, err
	}

	return &ReducerResult{
		RequestID: requestID,
		Timestamp: timestamp,
		Result:    outcome,
	}, nil
}

// readReducerOutcome reads a ReducerOutcome from a BSATN reader.
// Sum type: tag 0 = Ok(ReducerOk), tag 1 = OkEmpty, tag 2 = Err(Bytes), tag 3 = InternalError(String).
func readReducerOutcome(r bsatn.Reader) (ReducerOutcome, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	switch tag {
	case 0: // Ok(ReducerOk)
		retValue, err := bsatn.ReadByteArray(r)
		if err != nil {
			return nil, err
		}
		txUpdate, err := readTransactionUpdate(r)
		if err != nil {
			return nil, err
		}
		return &ReducerOk{
			RetValue:          retValue,
			TransactionUpdate: txUpdate,
		}, nil
	case 1: // OkEmpty (unit variant)
		return &ReducerOkEmpty{}, nil
	case 2: // Err(Bytes)
		errBytes, err := bsatn.ReadByteArray(r)
		if err != nil {
			return nil, err
		}
		return &ReducerErr{ErrorBytes: errBytes}, nil
	case 3: // InternalError(String)
		msg, err := r.GetString()
		if err != nil {
			return nil, err
		}
		return &ReducerInternalError{Message: msg}, nil
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "ReducerOutcome"}
	}
}

// readProcedureResult reads a ProcedureResult from a BSATN reader.
// Fields (Rust declaration order): status, timestamp, total_host_execution_duration, request_id.
func readProcedureResult(r bsatn.Reader) (*ProcedureResult, error) {
	status, err := readProcedureStatus(r)
	if err != nil {
		return nil, err
	}

	timestamp, err := types.ReadTimestamp(r)
	if err != nil {
		return nil, err
	}

	duration, err := types.ReadTimeDuration(r)
	if err != nil {
		return nil, err
	}

	requestID, err := r.GetU32()
	if err != nil {
		return nil, err
	}

	return &ProcedureResult{
		Status:                     status,
		Timestamp:                  timestamp,
		TotalHostExecutionDuration: duration,
		RequestID:                  requestID,
	}, nil
}

// readProcedureStatus reads a ProcedureStatus from a BSATN reader.
// Sum type: tag 0 = Returned(Bytes), tag 1 = InternalError(String).
func readProcedureStatus(r bsatn.Reader) (ProcedureStatus, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	switch tag {
	case 0: // Returned(Bytes)
		value, err := bsatn.ReadByteArray(r)
		if err != nil {
			return nil, err
		}
		return &ProcedureReturned{Value: value}, nil
	case 1: // InternalError(String)
		msg, err := r.GetString()
		if err != nil {
			return nil, err
		}
		return &ProcedureInternalError{Message: msg}, nil
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "ProcedureStatus"}
	}
}

// readQueryRows reads a QueryRows from a BSATN reader.
// Fields: tables(Vec<SingleTableRows>).
func readQueryRows(r bsatn.Reader) (*QueryRows, error) {
	tables, err := bsatn.ReadArray(r, readSingleTableRows)
	if err != nil {
		return nil, err
	}

	return &QueryRows{Tables: tables}, nil
}

// readSingleTableRows reads a SingleTableRows from a BSATN reader.
// Fields: table(RawIdentifier=String), rows(BsatnRowList).
func readSingleTableRows(r bsatn.Reader) (SingleTableRows, error) {
	tableName, err := r.GetString()
	if err != nil {
		return SingleTableRows{}, err
	}

	rows, err := ReadBsatnRowList(r)
	if err != nil {
		return SingleTableRows{}, err
	}

	return SingleTableRows{
		TableName: tableName,
		Rows:      rows,
	}, nil
}

// readQuerySetUpdate reads a QuerySetUpdate from a BSATN reader.
// Fields: query_set_id(QuerySetId{u32}), tables(Vec<TableUpdate>).
func readQuerySetUpdate(r bsatn.Reader) (QuerySetUpdate, error) {
	querySetID, err := r.GetU32()
	if err != nil {
		return QuerySetUpdate{}, err
	}

	tables, err := bsatn.ReadArray(r, readTableUpdate)
	if err != nil {
		return QuerySetUpdate{}, err
	}

	return QuerySetUpdate{
		QuerySetID: querySetID,
		Tables:     tables,
	}, nil
}

// readTableUpdate reads a TableUpdate from a BSATN reader.
// Fields: table_name(RawIdentifier=String), rows(Vec<TableUpdateRows>).
func readTableUpdate(r bsatn.Reader) (TableUpdate, error) {
	tableName, err := r.GetString()
	if err != nil {
		return TableUpdate{}, err
	}

	rows, err := bsatn.ReadArray(r, readTableUpdateRows)
	if err != nil {
		return TableUpdate{}, err
	}

	return TableUpdate{
		TableName: tableName,
		Rows:      rows,
	}, nil
}

// readTableUpdateRows reads a TableUpdateRows from a BSATN reader.
// Sum type: tag 0 = PersistentTable(PersistentTableRows), tag 1 = EventTable(EventTableRows).
func readTableUpdateRows(r bsatn.Reader) (TableUpdateRows, error) {
	tag, err := r.GetSumTag()
	if err != nil {
		return nil, err
	}

	switch tag {
	case 0: // PersistentTable
		inserts, err := ReadBsatnRowList(r)
		if err != nil {
			return nil, err
		}
		deletes, err := ReadBsatnRowList(r)
		if err != nil {
			return nil, err
		}
		return &PersistentTableRows{
			Inserts: inserts,
			Deletes: deletes,
		}, nil
	case 1: // EventTable
		events, err := ReadBsatnRowList(r)
		if err != nil {
			return nil, err
		}
		return &EventTableRows{Events: events}, nil
	default:
		return nil, &bsatn.ErrInvalidTag{Tag: tag, SumName: "TableUpdateRows"}
	}
}
