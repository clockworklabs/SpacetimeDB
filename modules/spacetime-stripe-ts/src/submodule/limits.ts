export const MAX_WEBHOOK_BODY_LENGTH = 1024 * 1024;
export const MAX_WEBHOOK_HEADER_LENGTH = 8192;
export const MAX_WEBHOOK_METADATA_LENGTH = 255;

// Stripe retries a delivery for up to three days; stored events are kept well
// past that so redeliveries stay deduplicated and failures can be replayed.
export const WEBHOOK_EVENT_RETENTION_MICROS =
  30n * 24n * 60n * 60n * 1_000_000n;
export const WEBHOOK_PRUNE_INTERVAL_MICROS = 60n * 60n * 1_000_000n;
export const WEBHOOK_PRUNE_BATCH = 500;
