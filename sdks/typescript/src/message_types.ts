import { Address, Identity } from ".";
import { TableUpdate } from "./table";

export class SubscriptionUpdateMessage {
  public tableUpdates: TableUpdate[];

  constructor(tableUpdates: TableUpdate[]) {
    this.tableUpdates = tableUpdates;
  }
}

export class TransactionUpdateEvent {
  public identity: Identity;
  public address: Address | null;
  public originalReducerName: string;
  public reducerName: string;
  public args: Uint8Array;
  public status: string;
  public message: string;

  constructor(
    identity: Identity,
    address: Address | null,
    originalReducerName: string,
    reducerName: string,
    args: Uint8Array,
    status: string,
    message: string
  ) {
    this.identity = identity;
    this.address = address;
    this.originalReducerName = originalReducerName;
    this.reducerName = reducerName;
    this.args = args;
    this.status = status;
    this.message = message;
  }
}

export class TransactionUpdateMessage {
  public tableUpdates: TableUpdate[];
  public event: TransactionUpdateEvent;

  constructor(tableUpdates: TableUpdate[], event: TransactionUpdateEvent) {
    this.tableUpdates = tableUpdates;
    this.event = event;
  }
}

export class IdentityTokenMessage {
  public identity: Identity;
  public token: string;
  public address: Address;

  constructor(identity: Identity, token: string, address: Address) {
    this.identity = identity;
    this.token = token;
    this.address = address;
  }
}
export type Message =
  | SubscriptionUpdateMessage
  | TransactionUpdateMessage
  | IdentityTokenMessage;
