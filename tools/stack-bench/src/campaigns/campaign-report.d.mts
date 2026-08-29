export interface GeneratedCampaignReport {
  report: { contentSha256: string };
  reportPath: string;
  htmlPath: string;
}

export function generateCampaignReport(directory: string): GeneratedCampaignReport;
