import { Identity } from "./identity";
import { Address } from "./address";

export class ReducerEvent {
  public callerIdentity: Identity;
  public callerAddress: Address | null;
  public reducerName: string;
  public status: string;
  public message: string;
  public args: any;

  constructor(
    callerIdentity: Identity,
    callerAddress: Address | null,
    reducerName: string,
    status: string,
    message: string,
    args: any
  ) {
    this.callerIdentity = callerIdentity;
    this.callerAddress = callerAddress;
    this.reducerName = reducerName;
    this.status = status;
    this.message = message;
    this.args = args;
  }
}
