export interface AvailableStock {
  warehouseId: bigint;
  quantity: number;
}

export interface StockAllocation {
  warehouseId: bigint;
  quantity: number;
}

export function planStockAllocation(
  rows: readonly AvailableStock[],
  quantity: number,
): StockAllocation[] | null {
  if (quantity < 1 || rows.reduce((sum, row) => sum + row.quantity, 0) < quantity) return null;

  let remaining = quantity;
  const allocations: StockAllocation[] = [];
  for (const row of rows) {
    if (remaining === 0) break;
    const allocated = Math.min(row.quantity, remaining);
    if (allocated === 0) continue;
    allocations.push({ warehouseId: row.warehouseId, quantity: allocated });
    remaining -= allocated;
  }
  return allocations;
}

export function isTicketCreator(senderIdentity: string, creatorIdentity: string): boolean {
  return senderIdentity === creatorIdentity;
}

export function hasPendingRestockForRule(
  rows: readonly { reorderRuleId?: bigint; status: string }[],
  ruleId: bigint,
): boolean {
  return rows.some(row => row.reorderRuleId === ruleId && row.status === 'pending');
}
