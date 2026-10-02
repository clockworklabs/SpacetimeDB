import spacetimedb from '../../src/submodule/index';
import { install } from '../../src/submodule/install';
export {
  addPresenceAdmin,
  clearPresence,
  heartbeat,
  presenceEntriesAdmin,
  presenceOnline,
  presenceSweep,
  removePresenceAdmin,
  runSweep,
  updateConfig,
} from '../../src/submodule/index';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  install(ctx);
});
