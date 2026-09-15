// Reference-only native SDK probe. Uses the current actor's own token. It never
// wraps a reducer in HTTP or borrows the publisher's credential.
export async function nativeRequest(page, operation, args = {}) {
  return page.evaluate(async ({operation,args}) => {
    const {DbConnection}=await import('/src/module_bindings/index.ts');
    const {SPACETIMEDB_URI,MODULE_NAME}=await import('/src/config.ts');
    let connection;
    const connect = new Promise((resolve,reject) => {
      connection=DbConnection.builder().withUri(SPACETIMEDB_URI).withDatabaseName(MODULE_NAME)
        .withToken(localStorage.getItem('auth_token')||undefined)
        .onConnect(resolve).onConnectError((_ctx,error)=>reject(error)).build();
    });
    let timer;
    const timeout=new Promise((_resolve,reject)=>{timer=setTimeout(()=>reject(new Error('Native observation timed out')),10000);});
    try {
      await Promise.race([connect,timeout]);
      const read = (query,table) => new Promise((resolve,reject)=>{
        const subscription=connection.subscriptionBuilder().onApplied(()=>{
          const rows=[...connection.db[table].iter()];
          subscription.unsubscribe();resolve(rows);
        }).onError(ctx=>reject(ctx.event??new Error('Native subscription failed'))).subscribe(query);
      });
      let value;
      if(operation==='profile') value=(await Promise.race([read('SELECT * FROM my_profile','myProfile'),timeout]))[0]??{name:'',address:''};
      else if(operation==='addresses') value=await Promise.race([read('SELECT * FROM my_addresses','myAddresses'),timeout]);
      else {
        const input={...args};
        if(input.id!==undefined)input.id=BigInt(input.id);
        if(input.itemId!==undefined)input.itemId=BigInt(input.itemId);
        await Promise.race([connection.reducers[operation](input),timeout]);
      }
      return {transport:'spacetimedb',operation,outcome:'committed',value:JSON.parse(JSON.stringify(value??null,(_key,v)=>typeof v==='bigint'?String(v):v))};
    } catch(error) {
      const message=String(error?.message??error);
      if(error?.name!=='SenderError') throw error;
      return {transport:'spacetimedb',operation,outcome:'rejected',error:message};
    } finally {clearTimeout(timer);connection?.disconnect();}
  },{operation,args});
}
