export { default } from './submodule/schema';
export {
  resendDeliveryEventTable,
  resendEmailTable,
  t,
} from './submodule/schema';
export { install } from './submodule/install';
export {
  ingestResendWebhook,
  replayWebhookEvent,
  makeResendWebhookHandler,
  type ResendWebhookIngestArgs,
} from './submodule/webhooks';
export {
  sendEmailRequest,
  sendEmail,
  cancelEmail,
  getEmail,
  listEmailsByUserId,
  listEmailsByOrgId,
  listEmailsByStatus,
  listDeliveryEventsForEmail,
  resendApiRequest,
  type SendEmailArgs,
} from './submodule/operations';

export { setResendConfig, getResendConfigStatus } from './submodule/config';
export { addAdminIdentity, removeAdminIdentity } from './submodule/auth';
