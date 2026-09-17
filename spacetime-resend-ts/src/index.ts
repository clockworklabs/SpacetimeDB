export { default, init } from './submodule/schema';
export { ingestResendWebhook, replayWebhookEvent } from './submodule/webhooks';

export { setResendConfig, getResendConfigStatus } from './submodule/config';
export { addAdminIdentity, removeAdminIdentity } from './submodule/auth';
export {
  cancelEmail,
  getEmail,
  listDeliveryEventsForEmail,
  listEmailsByOrgId,
  listEmailsByStatus,
  listEmailsByUserId,
  resendApiRequest,
  sendEmail,
} from './submodule/operations';
