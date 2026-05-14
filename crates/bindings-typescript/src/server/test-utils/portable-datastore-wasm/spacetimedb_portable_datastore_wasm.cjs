
let imports = {};
imports['__wbindgen_placeholder__'] = module.exports;

let cachedUint8ArrayMemory0 = null;

function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });

cachedTextDecoder.decode();

function decodeText(ptr, len) {
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_export_0.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let WASM_VECTOR_LEN = 0;

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}
/**
 * @enum {0 | 1}
 */
exports.WasmCommitMode = Object.freeze({
    Normal: 0, "0": "Normal",
    DropEventTableRows: 1, "1": "DropEventTableRows",
});

const WasmPortableDatastoreFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmportabledatastore_free(ptr >>> 0, 1));

class WasmPortableDatastore {

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmPortableDatastoreFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmportabledatastore_free(ptr, 0);
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @returns {number}
     */
    clearTable(tx, table_id) {
        _assertClass(tx, WasmPortableTransaction);
        const ret = wasm.wasmportabledatastore_clearTable(this.__wbg_ptr, tx.__wbg_ptr, table_id);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {WasmPortableTransaction} tx
     */
    rollbackTx(tx) {
        _assertClass(tx, WasmPortableTransaction);
        const ret = wasm.wasmportabledatastore_rollbackTx(this.__wbg_ptr, tx.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {WasmPortableTransaction}
     */
    beginMutTx() {
        const ret = wasm.wasmportabledatastore_beginMutTx(this.__wbg_ptr);
        return WasmPortableTransaction.__wrap(ret);
    }
    /**
     * @param {number} table_id
     * @returns {number}
     */
    tableRowCount(table_id) {
        const ret = wasm.wasmportabledatastore_tableRowCount(this.__wbg_ptr, table_id);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {number} table_id
     * @returns {Array<any>}
     */
    tableRowsBsatn(table_id) {
        const ret = wasm.wasmportabledatastore_tableRowsBsatn(this.__wbg_ptr, table_id);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @returns {number}
     */
    tableRowCountTx(tx, table_id) {
        _assertClass(tx, WasmPortableTransaction);
        const ret = wasm.wasmportabledatastore_tableRowCountTx(this.__wbg_ptr, tx.__wbg_ptr, table_id);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @param {Uint8Array} relation
     * @returns {number}
     */
    deleteByRelBsatn(tx, table_id, relation) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(relation, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_deleteByRelBsatn(this.__wbg_ptr, tx.__wbg_ptr, table_id, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @returns {Array<any>}
     */
    tableRowsBsatnTx(tx, table_id) {
        _assertClass(tx, WasmPortableTransaction);
        const ret = wasm.wasmportabledatastore_tableRowsBsatnTx(this.__wbg_ptr, tx.__wbg_ptr, table_id);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {string} payload
     * @param {string} connection_id_hex
     * @returns {WasmValidatedAuth}
     */
    validateJwtPayload(payload, connection_id_hex) {
        const ptr0 = passStringToWasm0(payload, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(connection_id_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_validateJwtPayload(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WasmValidatedAuth.__wrap(ret[0]);
    }
    /**
     * @param {number} index_id
     * @param {Uint8Array} point
     * @returns {Array<any>}
     */
    indexScanPointBsatn(index_id, point) {
        const ptr0 = passArray8ToWasm0(point, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_indexScanPointBsatn(this.__wbg_ptr, index_id, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {number} index_id
     * @param {Uint8Array} prefix
     * @param {number} prefix_elems
     * @param {Uint8Array} rstart
     * @param {Uint8Array} rend
     * @returns {Array<any>}
     */
    indexScanRangeBsatn(index_id, prefix, prefix_elems, rstart, rend) {
        const ptr0 = passArray8ToWasm0(prefix, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(rstart, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(rend, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_indexScanRangeBsatn(this.__wbg_ptr, index_id, ptr0, len0, prefix_elems, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} index_id
     * @param {Uint8Array} point
     * @returns {Array<any>}
     */
    indexScanPointBsatnTx(tx, index_id, point) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(point, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_indexScanPointBsatnTx(this.__wbg_ptr, tx.__wbg_ptr, index_id, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} index_id
     * @param {Uint8Array} prefix
     * @param {number} prefix_elems
     * @param {Uint8Array} rstart
     * @param {Uint8Array} rend
     * @returns {Array<any>}
     */
    indexScanRangeBsatnTx(tx, index_id, prefix, prefix_elems, rstart, rend) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(prefix, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(rstart, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(rend, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_indexScanRangeBsatnTx(this.__wbg_ptr, tx.__wbg_ptr, index_id, ptr0, len0, prefix_elems, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @param {Uint8Array} row
     * @returns {Uint8Array}
     */
    insertBsatnGeneratedCols(tx, table_id, row) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(row, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_insertBsatnGeneratedCols(this.__wbg_ptr, tx.__wbg_ptr, table_id, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} table_id
     * @param {number} index_id
     * @param {Uint8Array} row
     * @returns {Uint8Array}
     */
    updateBsatnGeneratedCols(tx, table_id, index_id, row) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(row, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_updateBsatnGeneratedCols(this.__wbg_ptr, tx.__wbg_ptr, table_id, index_id, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} index_id
     * @param {Uint8Array} point
     * @returns {number}
     */
    deleteByIndexScanPointBsatn(tx, index_id, point) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(point, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_deleteByIndexScanPointBsatn(this.__wbg_ptr, tx.__wbg_ptr, index_id, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {number} index_id
     * @param {Uint8Array} prefix
     * @param {number} prefix_elems
     * @param {Uint8Array} rstart
     * @param {Uint8Array} rend
     * @returns {number}
     */
    deleteByIndexScanRangeBsatn(tx, index_id, prefix, prefix_elems, rstart, rend) {
        _assertClass(tx, WasmPortableTransaction);
        const ptr0 = passArray8ToWasm0(prefix, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(rstart, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(rend, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_deleteByIndexScanRangeBsatn(this.__wbg_ptr, tx.__wbg_ptr, index_id, ptr0, len0, prefix_elems, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {Uint8Array} raw_module_def_bsatn
     * @param {string} module_identity_hex
     */
    constructor(raw_module_def_bsatn, module_identity_hex) {
        const ptr0 = passArray8ToWasm0(raw_module_def_bsatn, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(module_identity_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_new(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0] >>> 0;
        WasmPortableDatastoreFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    reset() {
        const ret = wasm.wasmportabledatastore_reset(this.__wbg_ptr);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} index_name
     * @returns {number}
     */
    indexId(index_name) {
        const ptr0 = passStringToWasm0(index_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_indexId(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {string} table_name
     * @returns {number}
     */
    tableId(table_name) {
        const ptr0 = passStringToWasm0(table_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmportabledatastore_tableId(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] >>> 0;
    }
    /**
     * @param {WasmPortableTransaction} tx
     * @param {WasmCommitMode} mode
     */
    commitTx(tx, mode) {
        _assertClass(tx, WasmPortableTransaction);
        const ret = wasm.wasmportabledatastore_commitTx(this.__wbg_ptr, tx.__wbg_ptr, mode);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
}
if (Symbol.dispose) WasmPortableDatastore.prototype[Symbol.dispose] = WasmPortableDatastore.prototype.free;

exports.WasmPortableDatastore = WasmPortableDatastore;

const WasmPortableTransactionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmportabletransaction_free(ptr >>> 0, 1));

class WasmPortableTransaction {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WasmPortableTransaction.prototype);
        obj.__wbg_ptr = ptr;
        WasmPortableTransactionFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmPortableTransactionFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmportabletransaction_free(ptr, 0);
    }
}
if (Symbol.dispose) WasmPortableTransaction.prototype[Symbol.dispose] = WasmPortableTransaction.prototype.free;

exports.WasmPortableTransaction = WasmPortableTransaction;

const WasmValidatedAuthFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmvalidatedauth_free(ptr >>> 0, 1));

class WasmValidatedAuth {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(WasmValidatedAuth.prototype);
        obj.__wbg_ptr = ptr;
        WasmValidatedAuthFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmValidatedAuthFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmvalidatedauth_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    get senderHex() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmvalidatedauth_senderHex(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string | undefined}
     */
    get connectionIdHex() {
        const ret = wasm.wasmvalidatedauth_connectionIdHex(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
}
if (Symbol.dispose) WasmValidatedAuth.prototype[Symbol.dispose] = WasmValidatedAuth.prototype.free;

exports.WasmValidatedAuth = WasmValidatedAuth;

exports.__wbg_new_1f3a344cf3123716 = function() {
    const ret = new Array();
    return ret;
};

exports.__wbg_newfromslice_074c56947bd43469 = function(arg0, arg1) {
    const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
    return ret;
};

exports.__wbg_push_330b2eb93e4e1212 = function(arg0, arg1) {
    const ret = arg0.push(arg1);
    return ret;
};

exports.__wbg_wbindgenthrow_451ec1a8469d7eb6 = function(arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1));
};

exports.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
    // Cast intrinsic for `Ref(String) -> Externref`.
    const ret = getStringFromWasm0(arg0, arg1);
    return ret;
};

exports.__wbindgen_init_externref_table = function() {
    const table = wasm.__wbindgen_export_0;
    const offset = table.grow(4);
    table.set(0, undefined);
    table.set(offset + 0, undefined);
    table.set(offset + 1, null);
    table.set(offset + 2, true);
    table.set(offset + 3, false);
    ;
};

const wasmPath = `${__dirname}/spacetimedb_portable_datastore_wasm_bg.wasm`;
const wasmBytes = require('fs').readFileSync(wasmPath);
const wasmModule = new WebAssembly.Module(wasmBytes);
const wasm = exports.__wasm = new WebAssembly.Instance(wasmModule, imports).exports;

wasm.__wbindgen_start();

