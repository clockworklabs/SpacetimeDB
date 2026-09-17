export { default } from './submodule/schema';
export {
  resendDeliveryEventTable,
  resendEmailTable,
  t,
} from './submodule/schema';
export { installResend } from './submodule/install';
export * from './submodule/webhooks';
export * from './submodule/operations';

export { setResendConfig, getResendConfigStatus } from './submodule/config';
export { addAdminIdentity, removeAdminIdentity } from './submodule/auth';
