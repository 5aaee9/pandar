'use client'

import { LogOutIcon, UserRoundIcon } from 'lucide-react'
import { useTranslations } from 'next-intl'

import { Button } from '../components/ui/button'
import type { AuthMetadata } from './dashboard-types'
import { formatAuthSource } from './dashboard-runtime-helpers'
import { logoutHref } from './dashboard-shell'
import { SettingsSection } from './settings-section'

export function SettingsAccountSection({ auth }: { auth: AuthMetadata }) {
  const t = useTranslations('settingsPage')
  const tAuth = useTranslations('runtime.authSource')
  const signOutHref = logoutHref(auth)

  return (
    <SettingsSection
      description={t('accountDescription')}
      icon={UserRoundIcon}
      id="account"
      title={t('accountTitle')}
    >
      <div className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="text-sm font-medium text-foreground">
            {t('currentSession')}
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            {t('authenticatedWith', {
              source: formatAuthSource(auth.source, tAuth),
              provider: auth.provider,
            })}
          </p>
        </div>
        {signOutHref ? (
          <Button
            nativeButton={false}
            render={<a aria-label={t('signOut')} href={signOutHref} />}
            size="lg"
            variant="outline"
          >
            <LogOutIcon aria-hidden="true" />
            {t('signOut')}
          </Button>
        ) : (
          <span className="text-xs text-muted-foreground">
            {t('signOutUnavailable')}
          </span>
        )}
      </div>
    </SettingsSection>
  )
}
