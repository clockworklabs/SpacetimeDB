interface PassChipProps {
  passed?: boolean | null
  label?: string
  size?: 'sm' | 'md'
}

export default function PassChip({ passed, label, size = 'md' }: PassChipProps) {
  const paddingClass = size === 'sm' ? 'px-1.5 py-0.5 text-xs' : 'px-2 py-0.5 text-xs'

  if (passed === null || passed === undefined) {
    return (
      <span
        className={`inline-block rounded font-mono font-semibold ${paddingClass}`}
        style={{ backgroundColor: '#1e2a38', color: '#64748b' }}
      >
        {label ?? '—'}
      </span>
    )
  }

  return (
    <span
      className={`inline-block rounded font-mono font-semibold ${paddingClass}`}
      style={{
        backgroundColor: passed ? 'rgba(56,211,159,0.15)' : 'rgba(255,107,107,0.15)',
        color: passed ? '#38d39f' : '#ff6b6b',
        border: `1px solid ${passed ? 'rgba(56,211,159,0.3)' : 'rgba(255,107,107,0.3)'}`,
      }}
    >
      {label ?? (passed ? 'PASS' : 'FAIL')}
    </span>
  )
}
