/**
 * A unique public identifier for a user connected to a database.
 */
export declare class Identity {
    private data;
    /**
     * Creates a new `Identity`.
     */
    constructor(data: string | Uint8Array);
    /**
     * Compare two identities for equality.
     */
    isEqual(other: Identity): boolean;
    /**
     * Print the identity as a hexadecimal string.
     */
    toHexString(): string;
    toUint8Array(): Uint8Array;
    /**
     * Parse an Identity from a hexadecimal string.
     */
    static fromString(str: string): Identity;
}
