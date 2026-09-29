import spacetimedb from '../../src/submodule/schema';
import { install } from '../../src/submodule/install';
export {
  adminRateLimitBuckets,
  addRateLimitAdmin,
  removeRateLimitAdmin,
  rateLimitSweep,
  resetBuckets,
  runSweep,
  updateConfig,
} from '../../src/submodule/operations';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  install(ctx);
});
