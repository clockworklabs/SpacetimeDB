import {DBConnectionBuilder, DBConnectionBase, type CallbackInit, DbContext} from '@clockworklabs/spacetimedb-sdk'
import {Message} from './message.ts';
import {User} from './user.ts'

import {SetNameReducer} from './set_name_reducer.ts'
import {SendMessageReducer} from './send_message_reducer.ts'

 class EventContext extends DbContext<RemoteTables, RemoteReducers> {}

 class RemoteTables {
  get user() {
    return User;
  }

  get message() {
    return Message;
  }
}

 class RemoteReducers  {
  #setNameReducer: SetNameReducer<EventContext, RemoteTables, RemoteReducers>;
  #sendMessageReducer: SendMessageReducer<EventContext, RemoteTables, RemoteReducers>;

  constructor(client: DBConnectionBase) {
    // All the reducers to be initialized here
    this.#setNameReducer = new SetNameReducer(client);
    this.#sendMessageReducer = new SendMessageReducer(client);
  }

  setName(name:string) {
    return this.#setNameReducer.call(name)
  }

  onSetName(callback: (ctx: EventContext, name: string) => void, init?: CallbackInit) {
    this.#setNameReducer.on(callback, init);
  }
  
  sendMessage(message:string) {
    return this.#sendMessageReducer.call(message)
  }

  onSendMessage(callback: (ctx: EventContext, text: string) => void, init?: CallbackInit) {
    this.#sendMessageReducer.on(callback, init);
  }
}

export class DbConnection {
  static builder() {
    const base = new DBConnectionBase();

    const tables = new RemoteTables();
    const reducers = new RemoteReducers(base);

    const builder = new DBConnectionBuilder(base, tables, reducers);

    return builder
  }
}
