export interface CampaignLock {
  path: string;
  token: string;
  record: Record<string, unknown>;
}

export function acquireCampaignLock(directory: string,
  campaign: { id: string; contentSha256: string }): CampaignLock;
export function releaseCampaignLock(lock: CampaignLock): boolean;
export function campaignLockIsActive(directory: string,
  campaign: { id: string; contentSha256: string }): boolean;
