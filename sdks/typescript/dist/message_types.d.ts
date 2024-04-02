import { Address, Identity } from ".";
import { TableUpdate } from "./table";
export declare class SubscriptionUpdateMessage {
    tableUpdates: TableUpdate[];
    constructor(tableUpdates: TableUpdate[]);
}
export declare class TransactionUpdateEvent {
    identity: Identity;
    address: Address | null;
    originalReducerName: string;
    reducerName: string;
    args: any[] | Uint8Array;
    status: string;
    message: string;
    constructor(identity: Identity, address: Address | null, originalReducerName: string, reducerName: string, args: any[] | Uint8Array, status: string, message: string);
}
export declare class TransactionUpdateMessage {
    tableUpdates: TableUpdate[];
    event: TransactionUpdateEvent;
    constructor(tableUpdates: TableUpdate[], event: TransactionUpdateEvent);
}
export declare class IdentityTokenMessage {
    identity: Identity;
    token: string;
    address: Address;
    constructor(identity: Identity, token: string, address: Address);
}
export type Message = SubscriptionUpdateMessage | TransactionUpdateMessage | IdentityTokenMessage;
