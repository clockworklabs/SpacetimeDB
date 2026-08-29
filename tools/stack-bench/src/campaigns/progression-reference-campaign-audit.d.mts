export interface ReferenceCampaignAudit {
  ok: boolean;
  [key: string]: unknown;
}

export function auditProgressionReferenceCampaign(directory: string): ReferenceCampaignAudit | null;
export function formatProgressionReferenceCampaignAudit(report: ReferenceCampaignAudit): string;
