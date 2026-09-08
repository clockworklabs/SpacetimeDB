import spacetimedb from '../../src/submodule/schema';
import { installRateLimit } from '../../src/submodule/install';
export {
  adminRateLimitBuckets,
  addRateLimitAdmin,
  consume,
  rate_limit_sweep,
  resetBuckets,
  runSweep,
  updateConfig,
} from '../../src/submodule/operations';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  installRateLimit(ctx);
});
