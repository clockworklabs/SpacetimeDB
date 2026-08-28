export interface MutationDefinition {
  id?: unknown;
  [key: string]: unknown;
}

export interface MutationEdit {
  file: string;
  find: string;
}

export interface MutationValidationIssue {
  kind: string;
  mutation?: string;
}

export function mutationFileEdits(mutation: MutationDefinition): MutationEdit[];
export function resolveMutationFile(appDirectory: string, relativePath: string): string;
export function validateMutationDefinitions(mutations: MutationDefinition[] | undefined): {
  issues: MutationValidationIssue[];
};
