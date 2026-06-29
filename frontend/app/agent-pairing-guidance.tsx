'use client'

import { useTranslations } from 'next-intl'

import { CreateAgentPairingForm } from './admin-settings-panel'
import type { Tenant } from './dashboard-types'

export function AgentPairingGuidance({
  selectedTenant,
  restricted,
}: {
  selectedTenant: Tenant | null
  restricted: boolean
}) {
  const t = useTranslations('agentPairing')

  return (
    <section className="overflow-hidden rounded-md border border-slate-300 bg-white">
      <div className="grid gap-0 lg:grid-cols-[minmax(0,1fr)_minmax(320px,0.72fr)]">
        <div className="border-b border-slate-200 px-4 py-4 lg:border-b-0 lg:border-r">
          <div>
            <h2 className="text-base font-semibold text-slate-950">{t('title')}</h2>
            <p className="mt-0.5 text-sm text-slate-600">
              {selectedTenant
                ? restricted
                  ? t('subtitleRestricted')
                  : t('subtitleTenant', {
                      name: selectedTenant.display_name,
                      slug: selectedTenant.slug,
                    })
                : t('subtitleNone')}
            </p>
          </div>
          <p className="mt-3 max-w-3xl text-sm text-slate-700">{t('summary')}</p>
          <div className="mt-4">
            <div className="text-xs font-medium text-slate-500">{t('stepsTitle')}</div>
            <ol className="mt-2 grid gap-2 text-sm text-slate-700">
              <li className="flex gap-2">
                <StepNumber value="1" />
                <span>
                  {selectedTenant
                    ? t('stepCreate', { name: selectedTenant.display_name })
                    : t('stepSelectTenant')}
                </span>
              </li>
              <li className="flex gap-2">
                <StepNumber value="2" />
                <span>{t('stepCopy')}</span>
              </li>
              <li className="flex gap-2">
                <StepNumber value="3" />
                <span>{t('stepStart')}</span>
              </li>
            </ol>
          </div>
        </div>
        <div className="bg-slate-50 px-4 py-4">
          {selectedTenant && !restricted ? (
            <CreateAgentPairingForm tenantId={selectedTenant.id} />
          ) : (
            <div className="text-sm">
              <div className="font-medium text-slate-950">
                {selectedTenant ? t('restrictedTitle') : t('noTenantTitle')}
              </div>
              <p className="mt-1 text-slate-600">
                {selectedTenant ? t('restrictedDetail') : t('noTenantDetail')}
              </p>
            </div>
          )}
        </div>
      </div>
    </section>
  )
}

function StepNumber({ value }: { value: string }) {
  return (
    <span className="mt-0.5 inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-slate-300 bg-slate-50 text-[11px] font-medium tabular-nums text-slate-700">
      {value}
    </span>
  )
}
