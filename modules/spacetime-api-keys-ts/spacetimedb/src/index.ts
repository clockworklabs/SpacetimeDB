import spacetimedb from '../../src/submodule/schema';
import { install } from '../../src/submodule/install';
export {
  addApiKeysAdmin,
  apiKeyUsageAdmin,
  apiKeysAdmin,
  apiKeysSweep,
  createApiKey,
  createApiKeyForSubject,
  myApiKeys,
  removeApiKeysAdmin,
  revokeApiKey,
  revokeApiKeyForSubject,
  rotateApiKey,
  setApiKeysConfig,
} from '../../src/submodule/operations';

export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  install(ctx);
});
