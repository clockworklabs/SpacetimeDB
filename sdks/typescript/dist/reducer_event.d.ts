import { Identity } from "./identity";
import { Address } from "./address";
export declare class ReducerEvent {
    callerIdentity: Identity;
    callerAddress: Address | null;
    reducerName: string;
    status: string;
    message: string;
    args: any;
    constructor(callerIdentity: Identity, callerAddress: Address | null, reducerName: string, status: string, message: string, args: any);
}
