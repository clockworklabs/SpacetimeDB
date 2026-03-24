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
        style={{ backgroundColor: '#202126', color: '#6f7987' }}
      >
        {label ?? '—'}
      </span>
    )
  }

  return (
    <span
      className={`inline-block rounded font-mono font-semibold ${paddingClass}`}
      style={{
        backgroundColor: passed ? 'rgba(76,244,144,0.12)' : 'rgba(255,76,76,0.12)',
        color: passed ? '#4cf490' : '#ff4c4c',
        border: `1px solid ${passed ? 'rgba(76,244,144,0.3)' : 'rgba(255,76,76,0.3)'}`,
      }}
    >
      {label ?? (passed ? 'PASS' : 'FAIL')}
    </span>
  )
}
