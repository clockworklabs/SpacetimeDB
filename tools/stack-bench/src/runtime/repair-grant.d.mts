import type { Artifact } from '../evidence/artifacts.js';

export interface RepairParentPayload {
  id: string;
}

export interface RepairCheckpointPayload {
  source: { sha256: string };
}

export interface InspectedRepairParent {
  root: string;
  parent: RepairParentPayload;
  parentArtifact: Artifact;
  level: { score: number; max: number };
  checkpoint: Artifact<RepairCheckpointPayload>;
  cumulativeRoundsBefore: number;
  configuration: { buildImage?: string | null };
}

export function inspectRepairParent(parentDirectory: string, levelNumber: number): InspectedRepairParent;
export function createRepairGrant(parentDirectory: string,
  grant: { level: number; rounds: number }): InspectedRepairParent;
