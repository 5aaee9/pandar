import { useTranslations } from 'next-intl'

import type { SecretActionState } from './actions'

export function SecretActionResult({ state }: { state: SecretActionState }) {
  const t = useTranslations('admin')
  if (!state) {
    return null
  }
  if (!state.ok) {
    return <div className="rounded border border-red-200 bg-red-50 px-2 py-1 text-xs text-red-900">{state.error}</div>
  }
  if (state.kind === 'tenant_token') {
    return (
      <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{state.token}</code>
        <div>{t('tokenShownOnce')}</div>
      </div>
    )
  }
  if (state.kind === 'join_link') {
    return (
      <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
        <div className="font-semibold">{state.message}</div>
        <code className="break-all rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{`/join#${state.token}`}</code>
        <div>{t('joinTokenShownOnce')}</div>
      </div>
    )
  }
  return (
    <div className="grid gap-1 rounded border border-amber-200 bg-amber-50 px-2 py-2 text-xs text-amber-950">
      <div className="font-semibold">{state.message}</div>
      <pre className="overflow-x-auto rounded bg-white px-2 py-1 font-mono text-[11px] text-slate-950">{state.agentEnv}</pre>
      <div>{t('pairingShownOnce')}</div>
    </div>
  )
}

export function Input({ name, label, defaultValue, placeholder, type = 'text' }: { name: string; label: string; defaultValue?: string; placeholder?: string; type?: string }) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-slate-500">{label}</span>
      <input className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950" defaultValue={defaultValue} name={name} placeholder={placeholder} type={type} />
    </label>
  )
}

export function Select({ name, label, values }: { name: string; label: string; values: string[] }) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-slate-500">{label}</span>
      <select className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950" name={name}>
        {values.map((value) => <option key={value} value={value}>{value}</option>)}
      </select>
    </label>
  )
}

export function PrimaryButton({ label }: { label: string }) {
  return <button className="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/80" type="submit">{label}</button>
}

export function Subhead({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex items-center justify-between border-b border-slate-200 px-4 py-2">
      <h3 className="text-sm font-semibold text-slate-950">{title}</h3>
      <span className="text-xs text-slate-600">{meta}</span>
    </div>
  )
}
