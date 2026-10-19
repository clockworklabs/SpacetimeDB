export { default, init } from './submodule/schema.js';
export {
  ingestResendWebhook,
  replayWebhookEvent,
} from './submodule/webhooks.js';

export { setResendConfig, getResendConfigStatus } from './submodule/config.js';
export { addAdminIdentity, removeAdminIdentity } from './submodule/auth.js';
export {
  cancelEmail,
  getEmail,
  getWebhookEvent,
  listDeliveryEventsForEmail,
  listEmailsByOrgId,
  listEmailsByStatus,
  listEmailsByUserId,
  resendApiRequest,
  sendEmail,
} from './submodule/operations.js';
