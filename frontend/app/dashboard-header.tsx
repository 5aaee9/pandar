import { useTranslations } from 'next-intl'

import { LanguageSwitcher } from '../components/language-switcher'
import type { Tenant } from './dashboard-types'

export function Header({
  apiUrl,
  tenants,
  selectedTenant,
}: {
  apiUrl: string
  tenants: Tenant[]
  selectedTenant: Tenant | null
}) {
  const t = useTranslations('header')
  return (
    <header className="flex flex-col gap-3 border-b border-slate-300 pb-4 md:flex-row md:items-end md:justify-between">
      <div>
        <h1 className="text-2xl font-semibold">{t('title')}</h1>
        <p className="mt-1 text-sm text-slate-600">{t('inventoryFrom', { apiUrl })}</p>
        <div className="mt-2">
          <LanguageSwitcher />
        </div>
      </div>
      {tenants.length > 1 ? (
        <form className="flex min-w-72 items-end gap-2" action="/">
          <label className="flex flex-1 flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-slate-500">{t('tenant')}</span>
            <select
              name="tenant"
              defaultValue={selectedTenant?.id}
              className="h-9 rounded-md border border-slate-300 bg-white px-2 text-sm text-slate-950"
            >
              {tenants.map((tenant) => (
                <option key={tenant.id} value={tenant.id}>
                  {tenant.display_name}
                </option>
              ))}
            </select>
          </label>
          <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">
            {t('view')}
          </button>
        </form>
      ) : null}
    </header>
  )
}
