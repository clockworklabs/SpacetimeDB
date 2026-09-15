import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';
import { attemptBrowserLaunchOptions } from '../../../dist/container/browser-pipe.js';
import { addressImportDifferences, migrationCheckoutDifferences } from '../../../dist/src/stacks/migration-state.js';
import { checkoutDifferences } from '../../../dist/src/stacks/checkout-state.js';
import { nativeRequest } from './m8-native-request.mjs';

async function openAddressBook(browser, url, name) {
  const context=await browser.newContext();
  const page=await context.newPage();
  page.setDefaultTimeout(10000);
  await page.goto(url,{waitUntil:'domcontentloaded'});
  const control=name=>page.locator(`[data-role="${name}"]`);
  if(name) {
    await control('signin-toggle').click();
    await control('signin-username').fill(name);
    await control('signin-password').fill('m8-baseline-password');
    await control('signin-submit').click();
    await control('current-user').filter({hasText:name}).waitFor();
    await control('profile-link').click();
    await control('address-book-link').click();
    await page.locator('[data-role="address-book"][data-loaded="true"]').waitFor();
  }
  return page;
}

const httpRequest=(page,backend,path,method='GET',body)=>page.evaluate(async ({path,method,body,mongo})=>{
  const token=mongo?localStorage.getItem('mongodb_shop_token'):null;
  const response=await fetch(path,{method,credentials:'include',headers:{'Content-Type':'application/json',...(token?{Authorization:`Bearer ${token}`}:{})},
    ...(body===undefined?{}:{body:JSON.stringify(body)})});
  return {status:response.status,body:await response.json()};
},{path,method,body,mongo:backend==='mongodb'});

// A focused observation can run again on an accepted migrated state, without
// replaying import/deletion setup or resetting the database.
export async function probeCrossAccountEdit({backend,url,id}) {
  const startedAtMs=Date.now();
  const browser=await chromium.launch({headless:true,...attemptBrowserLaunchOptions()});
  try {
    const page=await openAddressBook(browser,url,'admin-helper');
    if(backend==='spacetime') return {...await nativeRequest(page,'editAddress',{id,name:'intruder',address:'intruder'}),startedAtMs,completedAtMs:Date.now()};
    const result=await httpRequest(page,backend,`/api/addresses/${encodeURIComponent(id)}`,'PUT',{name:'intruder',address:'intruder'});
    assert([200,403,404].includes(result.status),`unexpected ownership probe status ${result.status}`);
    return {...result,outcome:result.status===200?'committed':'rejected',startedAtMs,completedAtMs:Date.now()};
  } finally {await browser.close();}
}

export async function qualifyAddressBook({ audit, before, snapshot, checkoutState, storedBooks, restart, save, url }) {
  const browser = await chromium.launch({ headless:true, ...attemptBrowserLaunchOptions() });
  const history = [];
  const mongo=audit.backend==='mongodb';
  const native=audit.backend==='spacetime';
  const reducer=async(page,operation,args)=>{
    const result=await nativeRequest(page,operation,args);
    history.push({phase:audit.phase,...result});return result;
  };
  const roles = '[data-role="';
  const control = (page, name) => page.locator(`${roles}${name}"]`);
  const phase = name => { audit.phase=name; console.log(`address-book: ${name}`); };
  const request = async (page,path,method='GET',body) => {
    const result = await httpRequest(page,audit.backend,path,method,body);
    history.push({phase:audit.phase,path,method,status:result.status,body:result.body});
    return result;
  };
  const open = name => openAddressBook(browser,url,name);
  const read = async (page, accountId) => {
    await control(page,'address-book-link').click();
    await page.locator('[data-role="address-book"][data-loaded="true"]').waitFor();
    const entries=await control(page,'address-entry').evaluateAll(elements => elements.map(element => ({
      id:element.getAttribute('data-address-id'),
      name:element.querySelector('[data-role="address-name"]').textContent,
      address:element.querySelector('[data-role="address-text"]').textContent,
      isDefault:element.getAttribute('data-default') === 'true',
    })));
    let profile;
    if(native) {const result=await reducer(page,'profile');assert.equal(result.outcome,'committed');profile=result.value;}
    else {const result=await request(page,mongo?'/api/progression/state':'/api/progression');assert.equal(result.status,200);profile=result.body.profile;}
    const observation={accountId,entries,legacyProfile:{name:profile?.name??'',address:profile?.address??''}};
    assert.deepEqual(observation,storedBooks().find(book=>book.accountId===accountId),'UI and stored address book differ');
    history.push({phase:audit.phase,observation});
    return observation;
  };
  const write = async (page, action) => {
    await action();
    await page.locator('[data-role="address-book"][data-submit-state="succeeded"]').waitFor();
  };
  const entry = (page,id) => page.locator(`[data-role="address-entry"][data-address-id="${id}"]`);
  const business = raw => {
    const actors=(mongo?before.state.users:before.state.account).filter(a=>['m8-customer','admin-helper'].includes(a.username));
    if(mongo) return {...raw,progressionprofiles:raw.progressionprofiles.filter(p=>!actors.some(a=>a._id.$oid===p.userId.$oid))};
    if(native) return {...raw,customer_profile:raw.customer_profile.filter(p=>!actors.some(a=>String(a.id)===String(p.account_id)))};
    return {...raw,account:raw.account.map(row=>{
      if(!actors.some(a=>a.id===row.id)) return row;
      const {profile_name,profile_address,...account}=row;return account;
    })};
  };
  const checkPreserved = () => {
    assert.deepEqual(business(snapshot().state),business(before.state),'unrelated business records changed');
    assert.deepEqual(migrationCheckoutDifferences(audit.beforeCheckout,
      ['m8-customer','admin-helper'].map(name=>checkoutState(name).state)),[]);
  };
  try {
    phase('initial-import');
    const accounts=mongo?before.state.users:before.state.account;
    const accountId=row=>mongo?row._id.$oid:String(row.id);
    const profiles=accounts.filter(row=>['m8-customer','admin-helper'].includes(row.username))
      .map(row=>{const p=mongo?before.state.progressionprofiles.find(p=>p.userId.$oid===accountId(row))
        :native?before.state.customer_profile.find(p=>String(p.account_id)===accountId(row)):row;
        return {accountId:accountId(row),name:mongo||native?p.name:p.profile_name,address:mongo||native?p.address:p.profile_address};});
    assert.equal(profiles.length,2);
    let a=await open('m8-customer'),b=await open('admin-helper');
    const aid=accountId(accounts.find(row=>row.username==='m8-customer'));
    const bid=accountId(accounts.find(row=>row.username==='admin-helper'));
    const initial=[await read(a,aid),await read(b,bid)];
    save('imported-addresses.json',initial);
    assert.deepEqual(addressImportDifferences(profiles,initial),[]);
    const importedId=initial[0].entries[0].id;
    checkPreserved();

    phase('add-edit-default');
    await control(a,'address-add').click();
    await control(a,'address-name-input').fill('Warehouse');
    await control(a,'address-text-input').fill(' 56 Café Road\nFloor 4 ');
    await write(a,()=>control(a,'address-save').click());
    let book=await read(a,aid);
    assert.equal(book.entries.length,2);
    const added=book.entries.find(value=>value.id !== importedId);
    assert(added && !added.isDefault);
    const addedId=added.id;
    await entry(a,addedId).locator('[data-role="address-edit"]').click();
    await control(a,'address-name-input').fill('Office');
    await control(a,'address-text-input').fill(' 78 Oak Road\nFloor 5 ');
    await write(a,()=>control(a,'address-save').click());
    await write(a,()=>entry(a,addedId).locator('[data-role="address-default"]').click());
    book=await read(a,aid);
    assert.deepEqual(book.entries.find(value=>value.id===addedId),{id:addedId,name:'Office',address:' 78 Oak Road\nFloor 5 ',isDefault:true});
    assert.equal(book.entries.filter(value=>value.isDefault).length,1);
    assert.deepEqual(book.legacyProfile,{name:'Office',address:' 78 Oak Road\nFloor 5 '});
    const changed=structuredClone(book);
    phase('repeat-start');
    await a.context().close(); await b.context().close();
    await restart();
    a=await open('m8-customer');b=await open('admin-helper');
    assert.deepEqual(await read(a,aid),changed,'address ID, edit or default changed after restart');
    assert.deepEqual(await read(b,bid),initial[1],'other owner changed');
    checkPreserved();

    phase('cross-account-access');
    const anon=await open();
    const ownerBefore=await read(a,aid);
    if(native) {
      for(const page of [b,anon]) {
        const view=await reducer(page,'addresses');assert.equal(view.outcome,'committed');
        assert(!view.value.some(e=>String(e.id)===addedId),'private address leaked to another actor');
        for(const [operation,args] of [['editAddress',{id:addedId,name:'intruder',address:'intruder'}],
          ['chooseAddress',{id:addedId}],['deleteAddress',{id:addedId}]]) {
          assert.equal((await reducer(page,operation,args)).outcome,'rejected',`unauthorized ${operation} committed`);
        }
      }
    } else {
    for(const [page,statuses] of [[b,[403,404]],[anon,[401,403]]]) {
      for(const [path,method,body] of [
        [`/api/addresses/${addedId}`,'GET'],
        [`/api/addresses/${addedId}`,'PUT',{name:'intruder',address:'intruder'}],
        [`/api/addresses/${addedId}/default`,'PUT'],
        [`/api/addresses/${addedId}`,'DELETE'],
      ]) {
        const response=await request(page,path,method,body);
        assert(statuses.includes(response.status),`unauthorized ${method} ${path} returned ${response.status}`);
        assert(!JSON.stringify(response.body).includes('78 Oak Road'),'private address leaked');
      }
    }
    assert([401,403].includes((await request(anon,'/api/addresses')).status));
    }
    assert.deepEqual(await read(a,aid),ownerBefore);
    assert.deepEqual(await read(b,bid),initial[1]);
    await anon.context().close();

    phase('legacy-profile-write');
    await control(a,'profile-name').fill('New recipient');
    await control(a,'profile-address').fill('90 Market Street');
    // Retain an unsaved edit across the reference's periodic refresh. Waiting
    // here tests form stability; it is not a retry of a failed write.
    await delay(1500);
    assert.equal(await control(a,'profile-name').inputValue(),'New recipient');
    assert.equal(await control(a,'profile-address').inputValue(),'90 Market Street');
    await control(a,'profile-save').click();
    await control(a,'profile-address-summary').filter({hasText:'90 Market Street'}).waitFor();
    book=await read(a,aid);
    assert.deepEqual(book.entries.find(value=>value.id===addedId),{id:addedId,name:'New recipient',address:'90 Market Street',isDefault:true});
    assert.deepEqual(book.entries.find(value=>value.id===importedId),initial[0].entries[0] && {...initial[0].entries[0],isDefault:false});

    phase('delete-and-restart');
    if(native) assert.equal((await reducer(a,'deleteAddress',{id:addedId})).outcome,'rejected','default deletion needs replacement');
    else {const denied=await request(a,`/api/addresses/${addedId}`,'DELETE');assert.equal(denied.status,400,'default deletion needs replacement');}
    assert.deepEqual(await read(a,aid),book,'refused delete mutated addresses');
    await write(a,()=>entry(a,importedId).locator('[data-role="address-delete"]').click());
    await write(a,()=>entry(a,addedId).locator('[data-role="address-delete"]').click());
    book=await read(a,aid);
    assert.deepEqual(book,{accountId:aid,entries:[],legacyProfile:{name:'',address:''}});
    await a.context().close();await b.context().close();await restart();
    a=await open('m8-customer');b=await open('admin-helper');
    assert.deepEqual(await read(a,aid),book,'deleted address resurrected');
    assert.deepEqual(await read(b,bid),initial[1]);
    checkPreserved();
    phase('legacy-write-empty-book');
    await control(a,'profile-name').fill('Fresh');await control(a,'profile-address').fill('102 Fresh Road');
    await control(a,'profile-save').click();
    await control(a,'profile-address-summary').filter({hasText:'102 Fresh Road'}).waitFor();
    const restored=await read(a,aid);
    assert.equal(restored.entries.length,1);assert.equal(restored.entries[0].isDefault,true);
    assert.equal(restored.entries[0].address,'102 Fresh Road');
    assert(![importedId,addedId].includes(restored.entries[0].id));
    checkPreserved();
    phase('purchase-after-migration');
    const prior=checkoutState('m8-customer').state;
    await a.reload({waitUntil:'domcontentloaded'});
    await control(a,'search-input').fill('Keyboard');await control(a,'search-input').press('Enter');
    if(native) assert.equal((await reducer(a,'addToCart',{itemId:prior.itemId})).outcome,'committed');
    else {
      const addedResponse=a.waitForResponse(response=>response.url().endsWith('/api/cart')&&response.request().method()==='POST');
      await control(a,'item-card').filter({hasText:'Keyboard'}).locator('[data-role="add-to-cart"]').click();
      assert.equal((await addedResponse).status(),200);
    }
    const prepared=checkoutState('m8-customer').state;
    if(native) assert.equal((await reducer(a,'checkout',{})).outcome,'committed');
    else {const result=await request(a,'/api/checkout','POST');assert.equal(result.status,200);}
    const purchased=checkoutState('m8-customer').state;
    const differences=checkoutDifferences(prior,prepared,purchased,1);
    save('post-migration-purchase.json',{prior,prepared,purchased,differences});
    assert.deepEqual(differences,[]);
    audit.addressBehaviorPassed=true;
  } finally { save('address-history.json',history); await browser.close(); }
}
