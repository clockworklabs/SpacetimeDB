import spacetimedb from '../../src/submodule/index';
import { install } from '../../src/submodule/install';
export {
  addAgentAdminIdentity,
  clearAgentOverride,
  clearApiKey,
  clearThreadLock,
  removeAgentAdminIdentity,
  setAgentConfig,
  setAgentOverride,
  setApiKey,
  threadLockSweep,
} from '../../src/submodule/index';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  install(ctx);
});
