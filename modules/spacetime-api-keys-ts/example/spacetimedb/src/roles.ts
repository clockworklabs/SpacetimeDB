// Scopes a share key can carry and the role each combination is shown as.
// The module and the browser client both import this file.

// view is read; the three edit scopes are the granular powers a share link
// can grant.
export const SCOPE_VIEW = 'colony:view';
export const SCOPE_TERRAFORM = 'colony:terraform';
export const SCOPE_BUILD = 'colony:build';
export const SCOPE_PLANT = 'colony:plant';

export function parseScopes(json: string): string[] {
  try {
    const parsed: unknown = JSON.parse(json);
    return Array.isArray(parsed) ? parsed.map(String) : [];
  } catch {
    return [];
  }
}

export function hasScope(scopes: string[], scope: string): boolean {
  return (
    scopes.includes('*') ||
    scopes.includes('colony:*') ||
    scopes.includes(scope)
  );
}

export function roleLabel(scopes: string[]): string {
  const canTerraform = hasScope(scopes, SCOPE_TERRAFORM);
  const canBuild = hasScope(scopes, SCOPE_BUILD);
  const canPlant = hasScope(scopes, SCOPE_PLANT);
  if (canTerraform && canBuild && canPlant) return 'Collaborator';
  if (canTerraform && !canBuild && !canPlant) return 'Terraformer';
  if (canBuild && !canTerraform && !canPlant) return 'Builder';
  if (canPlant && !canTerraform && !canBuild) return 'Planter';
  if (!canTerraform && !canBuild && !canPlant) return 'Viewer';
  return 'Editor';
}
