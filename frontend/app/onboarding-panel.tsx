import { getTranslations } from 'next-intl/server'

import { createTenantFromExternal } from './admin-actions'
import { authProviderConfig } from './auth-provider'
import type { MeResponse } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'
import { TenantAccessSwitcher } from './tenant-access-switcher'
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
          <TenantAccessSwitcher
            createAction={createTenantFromExternal}
            identityEmail={me.identity.email ?? t('noEmail')}
          />
        )}
      </section>
    </main>
  )
}

async function ProviderLinks({ signInUrl, signOutUrl }: { signInUrl: string | null; signOutUrl: string | null }) {
  if (!signInUrl && !signOutUrl) {
    return null
  }
  const t = await getTranslations('onboarding')

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
