// Stable error codes. Stripe API failures append `:<http status>` and Stripe's
// error details after the code.
export const errors = {
  notAuthorized: 'stripe.not_authorized',
  cannotRemoveLastAdmin: 'stripe.cannot_remove_last_admin',
  configNotSet: 'stripe.config_not_set',
  invalidWebhookSigningSecret: 'stripe.invalid_webhook_signing_secret',
  requestInvalid: 'stripe.request_invalid',
  requestPathInvalid: 'stripe.request_path_invalid',
  requestMethodInvalid: 'stripe.request_method_invalid',
  requestBodyTooLarge: 'stripe.request_body_too_large',
  idempotencyKeyTooLong: 'stripe.idempotency_key_too_long',
  createCustomerFailed: 'stripe.create_customer_failed',
  createCustomerInvalidResponse: 'stripe.create_customer_invalid_response',
  checkoutSessionRequiresItems: 'stripe.checkout_session_requires_items',
  checkoutSessionFailed: 'stripe.checkout_session_failed',
  checkoutSessionInvalidResponse: 'stripe.checkout_session_invalid_response',
  portalSessionFailed: 'stripe.portal_session_failed',
  portalSessionInvalidResponse: 'stripe.portal_session_invalid_response',
  subscriptionNotFound: 'stripe.subscription_not_found',
  subscriptionUpdateFailed: 'stripe.subscription_update_failed',
  subscriptionUpdateInvalidResponse:
    'stripe.subscription_update_invalid_response',
  subscriptionCancelFailed: 'stripe.subscription_cancel_failed',
  subscriptionCancelInvalidResponse:
    'stripe.subscription_cancel_invalid_response',
  subscriptionLookupFailed: 'stripe.subscription_lookup_failed',
  subscriptionLookupInvalidResponse:
    'stripe.subscription_lookup_invalid_response',
  subscriptionMissingLineItems: 'stripe.subscription_missing_line_items',
  subscriptionItemUpdateFailed: 'stripe.subscription_item_update_failed',
  subscriptionPayloadMissingFields:
    'stripe.subscription_payload_missing_fields',
  webhookMetadataInvalid: 'stripe.webhook_metadata_invalid',
  webhookPayloadTooLarge: 'stripe.webhook_payload_too_large',
  webhookSignatureTooLarge: 'stripe.webhook_signature_too_large',
  webhookSecretNotConfigured: 'stripe.webhook_secret_not_configured',
  webhookSignatureMismatch: 'stripe.webhook_signature_mismatch',
  webhookPayloadMissingMetadata: 'stripe.webhook_payload_missing_metadata',
  webhookMetadataMismatch: 'stripe.webhook_metadata_mismatch',
  webhookEventNotFound: 'stripe.webhook_event_not_found',
} as const;
