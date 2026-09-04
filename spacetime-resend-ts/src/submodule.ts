export { default } from './submodule/schema';
export {
  resendDeliveryEventTable,
  resendEmailTable,
  t,
} from './submodule/schema';
export { installResend } from './submodule/install';
export * from './submodule/webhooks';
export * from './submodule/operations';

export {
  set_resend_config,
  get_resend_config_status,
} from './submodule/config';
export { add_admin_identity, remove_admin_identity } from './submodule/auth';
