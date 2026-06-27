import { getTranslations } from 'next-intl/server'

import { createTenantFromExternal } from './actions'
import { authProviderConfig } from './auth-provider'
import type { MeResponse } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { LanguageSwitcher } from '../components/language-switcher'

export async function OnboardingPanel({ me }: { me: MeResponse }) {
  const t = await getTranslations('onboarding')
  const auth = authProviderConfig()

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-3xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader
          title={t('title')}
          subtitle={t('subtitle', { name: me.identity.display_name, provider: auth.provider })}
          meta={me.identity.email ?? t('noEmail')}
        />
        <div className="flex justify-end px-4 py-2">
          <LanguageSwitcher />
        </div>

        <ProviderLinks signInUrl={auth.signInUrl} signOutUrl={auth.signOutUrl} />

        {me.identity.email_verified !== true ? (
          <EmptyState
            title={t('verifiedTitle')}
            message={t('verifiedMessage')}
          />
        ) : (
          <div className="grid gap-4 px-4 py-4 md:grid-cols-2">
            <form action={createTenantFromExternal} className="grid gap-3">
              <div>
                <div className="text-sm font-semibold text-slate-950">{t('createTitle')}</div>
                <div className="mt-1 text-sm text-slate-600">{t('createMessage')}</div>
              </div>
              <Input name="display_name" label={t('tenantName')} />
              <Input name="slug" label={t('tenantSlug')} />
              <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">
                {t('createSubmit')}
              </button>
            </form>

            <div className="grid gap-3">
              <div>
                <div className="text-sm font-semibold text-slate-950">{t('joinTitle')}</div>
                <div className="mt-1 text-sm text-slate-600">{t('joinMessage')}</div>
              </div>
              <a className="inline-flex h-9 items-center justify-center rounded-md border border-slate-300 px-3 text-sm font-medium" href="/join">
                {t('openJoin')}
              </a>
            </div>
          </div>
        )}
      </section>
    </main>
  )
}

async function ProviderLinks({ signInUrl, signOutUrl }: { signInUrl: string | null; signOutUrl: string | null }) {
  const t = await getTranslations('onboarding')
  if (!signInUrl && !signOutUrl) {
    return null
  }

  return (
    <div className="flex flex-wrap gap-2 border-b border-slate-200 px-4 py-3">
      {signInUrl ? (
        <a className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium" href={signInUrl}>
          {t('signIn')}
        </a>
      ) : null}
      {signOutUrl ? (
        <a className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium" href={signOutUrl}>
          {t('signOut')}
        </a>
      ) : null}
    </div>
  )
}

function Input({ name, label }: { name: string; label: string }) {
  return (
    <label className="grid gap-1 text-sm">
      <span className="text-xs font-medium text-slate-500">{label}</span>
      <input className="h-9 rounded-md border border-slate-300 px-2 text-sm text-slate-950" name={name} required />
    </label>
  )
}
