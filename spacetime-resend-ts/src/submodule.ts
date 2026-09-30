export { default } from './submodule/schema.js';
export {
  resendDeliveryEventTable,
  resendEmailTable,
  t,
} from './submodule/schema.js';
export { install } from './submodule/install.js';
export { errors } from './submodule/errors.js';
export {
  ingestResendWebhook,
  replayWebhookEvent,
  makeResendWebhookHandler,
  type ResendWebhookIngestArgs,
} from './submodule/webhooks.js';
export {
  sendEmailRequest,
  sendEmail,
  cancelEmail,
  getEmail,
  getWebhookEvent,
  listEmailsByUserId,
  listEmailsByOrgId,
  listEmailsByStatus,
  listDeliveryEventsForEmail,
  resendApiRequest,
  type SendEmailArgs,
} from './submodule/operations.js';

export { setResendConfig, getResendConfigStatus } from './submodule/config.js';
export { addAdminIdentity, removeAdminIdentity } from './submodule/auth.js';
