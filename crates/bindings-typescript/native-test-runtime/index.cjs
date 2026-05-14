const native = require('./spacetimedb_test_runtime_node.node');

function identityToHex(identity) {
  if (typeof identity === 'bigint') {
    return identity.toString(16).padStart(64, '0');
  }
  if (identity && typeof identity.toHexString === 'function') {
    return identity.toHexString();
  }
  if (typeof identity === 'string') {
    return identity.startsWith('0x') ? identity.slice(2) : identity;
  }
  throw new TypeError('expected identity as bigint, string, or Identity-like object');
}

function normalizeBytes(bytes) {
  return Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes);
}

class NativeTx {
  constructor(id) {
    this.id = id;
  }
}

class NativeContext {
  constructor(inner) {
    this.inner = inner;
  }

  reset() {
    return this.inner.reset();
  }

  tableId(name) {
    return this.inner.tableId(name);
  }

  indexId(name) {
    return this.inner.indexId(name);
  }

  tableRowCount(target, tableId) {
    return this.inner.tableRowCount(txId(target), tableId);
  }

  tableRows(target, tableId) {
    return this.inner.tableRows(txId(target), tableId);
  }

  insertBsatn(target, tableId, row) {
    return this.inner.insertBsatn(txId(target), tableId, normalizeBytes(row));
  }

  deleteAllByEqBsatn(target, tableId, row) {
    return this.inner.deleteAllByEqBsatn(txId(target), tableId, normalizeBytes(row));
  }

  indexScanPointBsatn(target, indexId, point) {
    return this.inner.indexScanPointBsatn(txId(target), indexId, normalizeBytes(point));
  }

  indexScanRangeBsatn(target, indexId, prefix, prefixElems, rstartLen, rendLen) {
    return this.inner.indexScanRangeBsatn(
      txId(target),
      indexId,
      normalizeBytes(prefix),
      prefixElems,
      rstartLen,
      rendLen
    );
  }

  deleteByIndexScanPointBsatn(target, indexId, point) {
    return this.inner.deleteByIndexScanPointBsatn(txId(target), indexId, normalizeBytes(point));
  }

  deleteByIndexScanRangeBsatn(target, indexId, prefix, prefixElems, rstartLen, rendLen) {
    return this.inner.deleteByIndexScanRangeBsatn(
      txId(target),
      indexId,
      normalizeBytes(prefix),
      prefixElems,
      rstartLen,
      rendLen
    );
  }

  updateBsatn(target, tableId, indexId, row) {
    return this.inner.updateBsatn(txId(target), tableId, indexId, normalizeBytes(row));
  }

  runQuery(sql, databaseIdentity) {
    return this.inner.runQuery(sql, identityToHex(databaseIdentity));
  }

  beginTx() {
    return new NativeTx(this.inner.beginTx());
  }

  commitTx(tx) {
    return this.inner.commitTx(tx.id);
  }

  abortTx(tx) {
    return this.inner.abortTx(tx.id);
  }
}

function txId(target) {
  return target instanceof NativeTx ? target.id : null;
}

const runtime = {
  createContext(moduleDef, moduleIdentity) {
    return new NativeContext(native.createContext(normalizeBytes(moduleDef), identityToHex(moduleIdentity)));
  },
  validateJwtPayload(jwtPayload) {
    return BigInt(`0x${native.validateJwtPayload(jwtPayload)}`);
  },
};

globalThis.__spacetimedbTestRuntime = runtime;

module.exports = runtime;
