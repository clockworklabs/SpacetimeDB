import type { DbConnection } from '../module_bindings';
import { AddressBook } from '../AddressBook';

export function NativeAddressBook({ conn }: { conn: DbConnection }) {
  const load = async () => {
    await conn.reducers.openAddressBook({});
    return new Promise<{ entries: { id: string; name: string; address: string; isDefault: boolean }[] }>((resolve, reject) => {
      const timer = setTimeout(() => { handle.unsubscribe(); reject(new Error('Address read timed out')); }, 10000);
      const handle = conn.subscriptionBuilder().onApplied(() => {
        clearTimeout(timer);
        const entries = [...conn.db.myAddresses.iter()].map(entry => ({
          id: String(entry.id), name: entry.name, address: entry.address, isDefault: entry.isDefault,
        })).sort((a, b) => BigInt(a.id) < BigInt(b.id) ? -1 : 1);
        handle.unsubscribe(); resolve({ entries });
      }).onError(ctx => { clearTimeout(timer); reject(ctx.event ?? new Error('Address subscription failed')); }).subscribe('SELECT * FROM my_addresses');
    });
  };
  return <AddressBook actions={{ load,
    save: entry => entry.id
      ? conn.reducers.editAddress({ id: BigInt(entry.id), name: entry.name, address: entry.address })
      : conn.reducers.addAddress({ name: entry.name, address: entry.address }),
    remove: id => conn.reducers.deleteAddress({ id: BigInt(id) }),
    choose: id => conn.reducers.chooseAddress({ id: BigInt(id) }),
  }} />;
}
