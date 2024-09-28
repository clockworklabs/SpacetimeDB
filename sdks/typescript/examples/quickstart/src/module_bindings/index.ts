import {
  DBConnectionBase,
  DBConnectionBuilder,
  DbContext,
  ReducerEvent,
  STDBEvent,
  type CallbackInit,
} from '@clockworklabs/spacetimedb-sdk';
import { MessageTable } from './message.ts';
import { User } from './user.ts';

import { SendMessageReducer } from './send_message_reducer.ts';
import { SetNameReducer } from './set_name_reducer.ts';

class EventContext extends DbContext<RemoteTables, RemoteReducers> {
  event: STDBEvent<ReducerEvent>;

  constructor(
    client: DBConnectionBase,
    db: RemoteTables,
    reducers: RemoteReducers,
    event: STDBEvent<ReducerEvent>
  ) {
    super(client, db, reducers);
    this.event = event;
  }
}

class RemoteTables {
  #client: DBConnectionBase;

  constructor(client: DBConnectionBase) {
    this.#client = client;
  }

  get user() {
    return User;
  }

  get message() {
    return new MessageTable(this.#client);
  }
}

class RemoteReducers {
  #setNameReducer: SetNameReducer<EventContext, RemoteTables, RemoteReducers>;
  #sendMessageReducer: SendMessageReducer<
    EventContext,
    RemoteTables,
    RemoteReducers
  >;

  constructor(client: DBConnectionBase) {
    // All the reducers to be initialized here
    this.#setNameReducer = new SetNameReducer(client);
    this.#sendMessageReducer = new SendMessageReducer(client);
  }

  setName(name: string) {
    return this.#setNameReducer.call(name);
  }

  onSetName(
    callback: (ctx: EventContext, name: string) => void,
    init?: CallbackInit
  ) {
    this.#setNameReducer.on(callback, init);
  }

  sendMessage(message: string) {
    return this.#sendMessageReducer.call(message);
  }

  onSendMessage(
    callback: (ctx: EventContext, text: string) => void,
    init?: CallbackInit
  ) {
    this.#sendMessageReducer.on(callback, init);
  }
}

export interface RemoteDBContext
  extends DbContext<RemoteTables, RemoteReducers> {}

export class DbConnection {
  static builder() {
    const base = new DBConnectionBase();

    const tables = new RemoteTables();
    const reducers = new RemoteReducers(base);

    const builder = new DBConnectionBuilder(base, tables, reducers);

    return builder;
  }
}
