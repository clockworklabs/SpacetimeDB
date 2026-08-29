export interface CampaignDependencyStrikeGrant {
  attemptId: string;
  grantId: string;
  level: number;
  nodeIds: string[];
  strikes: number;
}

export function grantCampaignDependencyStrikes(directory: string,
  input: CampaignDependencyStrikeGrant): unknown;
