import { NextIntlClientProvider } from 'next-intl'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { JobHistory } from './dashboard-job-history'
import type { Job, Tenant } from './dashboard-types'
import { DashboardViewContent } from './dashboard-view-content'

const tenant: Tenant = {
  id: 'tenant-1',
  slug: 'tenant-one',
  display_name: 'Tenant One',
  created_at: '2026-07-15T00:00:00Z',
}

function job(overrides: Partial<Job> = {}): Job {
  return {
    id: 'job-1',
    printer_id: 'printer-1',
    agent_id: 'agent-1',
    artifact_id: 'artifact-1',
    command_id: 'command-1',
    status: 'succeeded',
    error: null,
    created_at: '2026-07-15T00:00:00Z',
    updated_at: '2026-07-15T00:00:00Z',
    print: {
      status: 'completed',
      printer_state: null,
      progress_percent: 100,
      remaining_time_minutes: 0,
      current_layer: 10,
      total_layers: 10,
      active_file: null,
      last_progress_percent: 100,
      last_layer: 10,
      error: null,
      started_at: '2026-07-15T00:00:00Z',
      finished_at: '2026-07-15T01:00:00Z',
      updated_at: '2026-07-15T01:00:00Z',
    },
    command: {
      id: 'command-1',
      kind: 'print_project_file',
      status: 'succeeded',
    },
    artifact: {
      id: 'artifact-1',
      tenant_id: 'tenant-1',
      filename: 'benchy.3mf',
      content_type: 'model/3mf',
      size_bytes: 42,
      metadata: null,
      created_at: '2026-07-15T00:00:00Z',
    },
    material: {
      ams_mapping: null,
      ams_mapping2: null,
      filament_usage: [],
    },
    ...overrides,
  }
}

function renderHistory({
  jobs = [job()],
  canManageJobs = true,
  onDeleteRedirect = vi.fn(),
}: {
  jobs?: Job[]
  canManageJobs?: boolean
  onDeleteRedirect?: (url: string) => void
} = {}) {
  return {
    onDeleteRedirect,
    ...render(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          canManageJobs={canManageJobs}
          agents={[]}
          jobs={jobs}
          nowMs={Date.parse('2026-07-15T01:00:00Z')}
          onDeleteRedirect={onDeleteRedirect}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>,
    ),
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('JobHistory row actions', () => {
  it('confirms the selected job and can cancel without a request', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    renderHistory()

    await user.click(screen.getByRole('button', { name: 'Delete benchy.3mf' }))

    expect(screen.getByRole('dialog')).toBeVisible()
    expect(
      screen.getByRole('heading', { name: 'Delete print job?' }),
    ).toBeVisible()
    expect(
      screen.getByText(
        'Delete benchy.3mf? This permanently removes its job history. Its uploaded artifact is removed only when no other job uses it.',
      ),
    ).toBeVisible()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('renders Reprint in a terminal Print Jobs row', () => {
    renderHistory()

    const reprint = screen.getByRole('button', {
      name: 'Reprint benchy.3mf',
    })
    expect(reprint).toBeEnabled()
    expect(reprint.closest('form')).toHaveFormValues({
      tenant_id: 'tenant-1',
      return_to: 'jobs',
      job_id: 'job-1',
    })
  })

  it('enables recovery actions and explains a persisted stalled job', () => {
    renderHistory({
      jobs: [
        job({
          print: {
            ...job().print,
            status: 'stalled',
            progress_percent: 0,
            current_layer: 0,
            started_at: null,
            finished_at: null,
            updated_at: null,
          },
        }),
      ],
    })

    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toBeEnabled()
    expect(
      screen.getByRole('button', { name: 'Reprint benchy.3mf' }),
    ).toBeEnabled()
    expect(screen.getByText('Stalled')).toBeVisible()
    expect(screen.getByText('Print did not start within 15 minutes')).toBeVisible()
  })

  it('disables deletion while a job may still be active', () => {
    renderHistory({
      jobs: [
        job({
          status: 'queued',
          print: {
            ...job().print,
            status: 'pending',
            progress_percent: 0,
            current_layer: 0,
            started_at: null,
            finished_at: null,
          },
          command: {
            id: 'command-1',
            kind: 'print_project_file',
            status: 'queued',
          },
        }),
      ],
    })

    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toBeDisabled()
    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toHaveAccessibleDescription(
      'Only finished, failed, cancelled, or stalled waiting jobs can be deleted.',
    )
    expect(
      screen.queryByRole('button', { name: 'Reprint benchy.3mf' }),
    ).not.toBeInTheDocument()
  })

  it('disables management actions without tenant-admin capability', () => {
    renderHistory({ canManageJobs: false })

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeDisabled()
    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toBeDisabled()
    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toHaveAccessibleDescription('Only tenant administrators can delete jobs.')
  })

  it('deletes through the job proxy and redirects with a success status', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    const { onDeleteRedirect } = renderHistory()

    await user.click(screen.getByRole('button', { name: 'Delete benchy.3mf' }))
    await user.click(screen.getByRole('button', { name: 'Delete job' }))

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        '/api/tenants/tenant-1/jobs/job-1',
        {
          method: 'DELETE',
        },
      )
    })
    expect(onDeleteRedirect).toHaveBeenCalledWith(
      '/jobs?tenant=tenant-1&status=job_deleted',
    )
  })

  it('keeps confirmation open when the Hub rejects deletion', async () => {
    const user = userEvent.setup()
    const onDeleteRedirect = vi.fn()
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(null, { status: 409 })),
    )
    renderHistory({ onDeleteRedirect })

    await user.click(screen.getByRole('button', { name: 'Delete benchy.3mf' }))
    await user.click(screen.getByRole('button', { name: 'Delete job' }))

    expect(
      await screen.findByText(
        'Unable to delete this job. It may still be active, or you may not have permission. Refresh and try again.',
      ),
    ).toBeVisible()
    expect(screen.getByRole('dialog')).toBeVisible()
    expect(onDeleteRedirect).not.toHaveBeenCalled()
  })

  it('keeps job management enabled for an authorized app API token', () => {
    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <DashboardViewContent
          view="jobs"
          auth={{
            source: 'app_api_token',
            cookieName: 'pandar_auth',
            provider: 'none',
            signInUrl: null,
            signOutUrl: null,
          }}
          selectedTenant={tenant}
          health={{
            printersTotal: 0,
            printersOnline: 0,
            agentsTotal: 0,
            agentsConnected: 0,
            jobsActive: 0,
            jobsFailed: 0,
          }}
          attentionItems={[]}
          topSeverity={null}
          liveState="idle"
          lastEventAt={null}
          fleetEmpty={false}
          printers={[]}
          agents={[]}
          jobs={[job()]}
          nowMs={Date.parse('2026-07-15T01:00:00Z')}
          selectedCommand={null}
          commandData={null}
          notifications={[]}
          users={[]}
          userIdentities={[]}
          tenantTokens={[]}
          joinLinks={[]}
          auditEvents={[]}
          adminUnavailable={true}
          adminLoadError={false}
          canManageJobs={true}
        />
      </NextIntlClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeEnabled()
    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toBeEnabled()
  })
})
