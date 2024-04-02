"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Address = void 0;
/**
 * A unique public identifier for a client connected to a database.
 */
class Address {
    data;
    /**
     * Creates a new `Address`.
     */
    constructor(data) {
        this.data = data;
    }
    isZero() {
        return this.data.every((b) => b == 0);
    }
    static nullIfZero(data) {
        let addr = new Address(data);
        if (addr.isZero()) {
            return null;
        }
        else {
            return addr;
        }
    }
    static random() {
        function randomByte() {
            return Math.floor(Math.random() * 255);
        }
        let data = new Uint8Array(16);
        for (let i = 0; i < 16; i++) {
            data[i] = randomByte();
        }
        return new Address(data);
    }
    /**
     * Compare two addresses for equality.
     */
    isEqual(other) {
        if (this.data.length !== other.data.length) {
            return false;
        }
        for (let i = 0; i < this.data.length; i++) {
            if (this.data[i] !== other.data[i]) {
                return false;
            }
        }
        return true;
    }
    /**
     * Print the address as a hexadecimal string.
     */
    toHexString() {
        return Array.prototype.map
            .call(this.data, (x) => ("00" + x.toString(16)).slice(-2))
            .join("");
    }
    toUint8Array() {
        return this.data;
    }
    /**
     * Parse an Address from a hexadecimal string.
     */
    static fromString(str) {
        let matches = str.match(/.{1,2}/g) || [];
        let data = Uint8Array.from(matches.map((byte) => parseInt(byte, 16)));
        return new Address(data);
    }
    static fromStringOrNull(str) {
        let addr = Address.fromString(str);
        if (addr.isZero()) {
            return null;
        }
        else {
            return addr;
        }
    }
}
exports.Address = Address;
