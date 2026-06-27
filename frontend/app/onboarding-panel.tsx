import { createTenantFromExternal } from './actions'
import { authProviderConfig } from './auth-provider'
import type { MeResponse } from './dashboard-types'
import { EmptyState, SectionHeader } from './dashboard-ui'

export function OnboardingPanel({ me }: { me: MeResponse }) {
  const auth = authProviderConfig()

  return (
    <main className="min-h-screen bg-slate-100 px-4 py-5 text-slate-950 sm:px-6 lg:px-8">
      <section className="mx-auto max-w-3xl overflow-hidden rounded-md border border-slate-300 bg-white">
        <SectionHeader
          title="Tenant onboarding"
          subtitle={`${me.identity.display_name} authenticated by ${auth.provider}`}
          meta={me.identity.email ?? 'No verified email'}
        />

        <ProviderLinks signInUrl={auth.signInUrl} signOutUrl={auth.signOutUrl} />

        {me.identity.email_verified !== true ? (
          <EmptyState
            title="Verified email required"
            message="Verify the email address in the external auth provider before creating or joining a tenant."
          />
        ) : (
          <div className="grid gap-4 px-4 py-4 md:grid-cols-2">
            <form action={createTenantFromExternal} className="grid gap-3">
              <div>
                <div className="text-sm font-semibold text-slate-950">Create tenant</div>
                <div className="mt-1 text-sm text-slate-600">The current external identity becomes tenant admin.</div>
              </div>
              <Input name="display_name" label="Tenant name" />
              <Input name="slug" label="Tenant slug" />
              <button className="h-9 rounded-md bg-cyan-700 px-3 text-sm font-medium text-white hover:bg-cyan-800" type="submit">
                Create tenant
              </button>
            </form>

            <div className="grid gap-3">
              <div>
                <div className="text-sm font-semibold text-slate-950">Join tenant</div>
                <div className="mt-1 text-sm text-slate-600">Open an invitation URL from a tenant admin, or paste the token on the join page.</div>
              </div>
              <a className="inline-flex h-9 items-center justify-center rounded-md border border-slate-300 px-3 text-sm font-medium" href="/join">
                Open join page
              </a>
            </div>
          </div>
        )}
      </section>
    </main>
  )
}

function ProviderLinks({ signInUrl, signOutUrl }: { signInUrl: string | null; signOutUrl: string | null }) {
  if (!signInUrl && !signOutUrl) {
    return null
  }

  return (
    <div className="flex flex-wrap gap-2 border-b border-slate-200 px-4 py-3">
      {signInUrl ? (
        <a className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium" href={signInUrl}>
          Sign in
        </a>
      ) : null}
      {signOutUrl ? (
        <a className="inline-flex h-8 items-center rounded-md border border-slate-300 px-3 text-sm font-medium" href={signOutUrl}>
          Sign out
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
