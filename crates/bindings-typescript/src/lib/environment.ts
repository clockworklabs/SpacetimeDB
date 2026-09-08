/** Read-only database environment access. Missing keys return null; empty values return "". */
export interface Environment {
  /** Reads the current transaction, or a short snapshot outside a procedure transaction. */
  get(key: string): string | null;
}
