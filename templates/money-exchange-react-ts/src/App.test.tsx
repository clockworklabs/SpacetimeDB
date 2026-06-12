import { render, screen } from '@testing-library/react';
import { Identity, Timestamp } from 'spacetimedb';
import { describe, expect, it } from 'vitest';
import { ActivityFeed } from './App';
import { formatMoney, parseMoney } from './money';

describe('money amounts', () => {
  it('formats integer cents as dollars', () => {
    expect(formatMoney(10_000n)).toBe('$100.00');
    expect(formatMoney(1_234n)).toBe('$12.34');
  });

  it('parses valid positive dollar entry', () => {
    expect(parseMoney('12.34')).toBe(1_234n);
    expect(parseMoney('5')).toBe(500n);
    expect(parseMoney('0')).toBeUndefined();
    expect(parseMoney('1.234')).toBeUndefined();
  });

  it('rejects amounts that cannot be represented as u64 cents', () => {
    expect(parseMoney('184467440737095516.15')).toBe(
      18_446_744_073_709_551_615n
    );
    expect(parseMoney('184467440737095516.16')).toBeUndefined();
    expect(parseMoney('184467440737095517.16')).toBeUndefined();
  });
});

describe('ActivityFeed', () => {
  it('shows private debit and credit entries', () => {
    const me = Identity.zero();
    const recipient = Identity.fromString(
      '0000000000000000000000000000000000000000000000000000000000000001'
    );
    render(
      <ActivityFeed
        directory={[{ identity: recipient, name: 'Ada' }]}
        changes={[
          {
            id: 1n,
            accountIdentity: me,
            counterpartyIdentity: recipient,
            direction: { tag: 'Debit' },
            amountCents: 1_234n,
            createdAt: Timestamp.fromDate(new Date('2026-01-01T00:00:00Z')),
          },
          {
            id: 2n,
            accountIdentity: me,
            counterpartyIdentity: recipient,
            direction: { tag: 'Credit' },
            amountCents: 500n,
            createdAt: Timestamp.fromDate(new Date('2026-01-02T00:00:00Z')),
          },
        ]}
      />
    );

    expect(screen.getByText('Sent to Ada')).toBeInTheDocument();
    expect(screen.getByText('-$12.34')).toBeInTheDocument();
    expect(screen.getByText('Received from Ada')).toBeInTheDocument();
    expect(screen.getByText('+$5.00')).toBeInTheDocument();
  });
});
