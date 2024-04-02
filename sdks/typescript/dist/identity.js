"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Identity = void 0;
// Helper function convert from string to Uint8Array
function hexStringToUint8Array(str) {
    let matches = str.match(/.{1,2}/g) || [];
    let data = Uint8Array.from(matches.map((byte) => parseInt(byte, 16)));
    return data;
}
// Helper function for converting Uint8Array to hex string
function uint8ArrayToHexString(array) {
    return Array.prototype.map
        .call(array, (x) => ("00" + x.toString(16)).slice(-2))
        .join("");
}
/**
 * A unique public identifier for a user connected to a database.
 */
class Identity {
    data;
    /**
     * Creates a new `Identity`.
     */
    constructor(data) {
        // we get a JSON with __identity_bytes when getting a token with a JSON API
        // and an Uint8Array when using BSATN
        this.data =
            data.constructor === Uint8Array
                ? uint8ArrayToHexString(data)
                : data;
    }
    /**
     * Compare two identities for equality.
     */
    isEqual(other) {
        return this.toHexString() === other.toHexString();
    }
    /**
     * Print the identity as a hexadecimal string.
     */
    toHexString() {
        return this.data;
    }
    toUint8Array() {
        return hexStringToUint8Array(this.toHexString());
    }
    /**
     * Parse an Identity from a hexadecimal string.
     */
    static fromString(str) {
        return new Identity(str);
    }
}
exports.Identity = Identity;
