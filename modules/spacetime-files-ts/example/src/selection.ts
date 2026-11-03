import type { Entry } from './rendering';

export class VaultSelection {
  readonly selected = new Set<string>();
  focusPath: string | null = null;
  private anchorPath: string | null = null;
  private renderedEntries: Entry[] = [];

  get entries(): readonly Entry[] {
    return this.renderedEntries;
  }

  setEntries(entries: readonly Entry[]): void {
    this.renderedEntries = [...entries];
  }

  setAnchor(path: string): void {
    this.anchorPath = path;
  }

  focus(path: string): boolean {
    this.anchorPath = path;
    if (this.focusPath === path) return false;
    this.focusPath = path;
    return true;
  }

  clearFocus(): void {
    this.focusPath = null;
  }

  toggle(path: string): void {
    if (this.selected.has(path)) this.selected.delete(path);
    else this.selected.add(path);
    this.anchorPath = path;
  }

  selectRange(path: string): void {
    const filePaths = this.renderedEntries
      .filter(entry => entry.type === 'file')
      .map(entry => entry.path);
    const anchorIndex = filePaths.indexOf(this.anchorPath ?? '');
    const targetIndex = filePaths.indexOf(path);
    if (anchorIndex < 0 || targetIndex < 0) {
      this.toggle(path);
      return;
    }
    for (
      let index = Math.min(anchorIndex, targetIndex);
      index <= Math.max(anchorIndex, targetIndex);
      index++
    ) {
      this.selected.add(filePaths[index]!);
    }
  }

  prune(
    validSelectedPaths: ReadonlySet<string>,
    validFocusPaths: ReadonlySet<string>
  ): void {
    for (const path of this.selected) {
      if (!validSelectedPaths.has(path)) this.selected.delete(path);
    }
    if (this.focusPath && !validFocusPaths.has(this.focusPath)) {
      this.focusPath = null;
    }
    if (this.anchorPath && !validSelectedPaths.has(this.anchorPath)) {
      this.anchorPath = null;
    }
  }
}
