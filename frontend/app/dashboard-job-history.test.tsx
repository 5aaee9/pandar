import { NextIntlClientProvider } from 'next-intl'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { JobHistory } from './dashboard-job-history'
import type { Job, Tenant } from './dashboard-types'

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
  selectedTenant = tenant,
  jobs = [job()],
  nowMs = 0,
  onClearRedirect = vi.fn(),
}: {
  selectedTenant?: Tenant | null
  jobs?: Job[]
  nowMs?: number
  onClearRedirect?: (url: string) => void
} = {}) {
  return {
    onClearRedirect,
    ...render(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          agents={[]}
          jobs={jobs}
          nowMs={nowMs}
          onClearRedirect={onClearRedirect}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={selectedTenant}
        />
      </NextIntlClientProvider>,
    ),
  }
}

describe('JobHistory actions', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('confirms clear, deletes through the tenant proxy, and redirects on success', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    const { onClearRedirect } = renderHistory()

    await user.click(screen.getByRole('button', { name: 'Clear' }))

    expect(screen.getByRole('dialog')).toBeVisible()
    expect(
      screen.getByRole('heading', { name: 'Clear terminal and stalled waiting jobs?' }),
    ).toBeVisible()
    expect(
      screen.getByText(
        'This removes 1 terminal or stalled waiting job. Running and other active jobs are kept.',
      ),
    ).toBeVisible()

    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith('/api/tenants/tenant-1/jobs', {
        method: 'DELETE',
      })
    })
    expect(onClearRedirect).toHaveBeenCalledWith(
      '/jobs?tenant=tenant-1&status=jobs_cleared',
    )
  })

  it('disables clear without a tenant or a backend-clearable terminal job', () => {
    const { rerender } = renderHistory({ selectedTenant: null })

    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled()

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          agents={[]}
          jobs={[
            job({
              status: 'queued',
              command: { id: 'command-1', kind: 'start_print', status: 'queued' },
            }),
          ]}
          nowMs={0}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled()
  })

  it('keeps the confirmation open and reports a failed clear request', async () => {
    const user = userEvent.setup()
    const onClearRedirect = vi.fn()
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 403 })))
    renderHistory({ onClearRedirect })

    await user.click(screen.getByRole('button', { name: 'Clear' }))
    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))

    expect(
      await screen.findByText('Unable to clear jobs. Check your permission and try again.'),
    ).toBeVisible()
    expect(screen.getByRole('dialog')).toBeVisible()
    expect(onClearRedirect).not.toHaveBeenCalled()
  })

  it('counts terminal and stalled waiting jobs while retaining other active jobs', async () => {
    const user = userEvent.setup()
    const stalled = job({
      id: 'job-stalled',
      updated_at: '2026-07-15T00:00:00Z',
      print: {
        ...job().print,
        status: 'pending',
        progress_percent: 0,
        current_layer: 0,
        started_at: null,
        finished_at: null,
        updated_at: null,
      },
    })
    const running = job({
      id: 'job-running',
      updated_at: '2026-07-15T00:00:00Z',
      print: {
        ...job().print,
        status: 'running',
        progress_percent: 10,
        current_layer: 1,
        finished_at: null,
        updated_at: '2026-07-15T00:00:00Z',
      },
    })
    const recentlyUpdated = job({
      id: 'job-recent',
      updated_at: '2026-07-15T00:00:00Z',
      print: {
        ...stalled.print,
        updated_at: '2026-07-15T00:10:00Z',
      },
    })
    renderHistory({
      jobs: [job({ id: 'job-terminal' }), stalled, running, recentlyUpdated],
      nowMs: Date.parse('2026-07-15T00:16:00Z'),
    })

    await user.click(screen.getByRole('button', { name: 'Clear' }))

    expect(
      screen.getByText(
        'This removes 2 terminal or stalled waiting jobs. Running and other active jobs are kept.',
      ),
    ).toBeVisible()
  })

  it('uses a strict fifteen-minute stalled threshold', () => {
    const stalled = job({
      updated_at: '2026-07-15T00:00:00Z',
      print: {
        ...job().print,
        status: 'pending',
        progress_percent: 0,
        current_layer: 0,
        started_at: null,
        finished_at: null,
        updated_at: null,
      },
    })
    const { rerender } = renderHistory({ jobs: [stalled] })

    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled()

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          agents={[]}
          jobs={[stalled]}
          nowMs={Date.parse('2026-07-15T00:15:00Z')}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled()

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          agents={[]}
          jobs={[stalled]}
          nowMs={Date.parse('2026-07-15T00:15:00.001Z')}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear' })).toBeEnabled()
  })

  it('matches the backend rule for failed jobs that never started printing', () => {
    const failedBeforePrint = job({
      status: 'failed',
      print: {
        ...job().print,
        status: 'pending',
        progress_percent: 0,
        current_layer: 0,
        started_at: null,
        finished_at: null,
      },
      command: { id: 'command-1', kind: 'print_project_file', status: 'failed' },
    })
    const { rerender } = renderHistory({ jobs: [failedBeforePrint] })

    expect(screen.getByRole('button', { name: 'Clear' })).toBeEnabled()

    rerender(
      <NextIntlClientProvider locale="en" messages={en}>
        <JobHistory
          agents={[]}
          jobs={[
            job({
              ...failedBeforePrint,
              print: { ...failedBeforePrint.print, progress_percent: 1 },
            }),
          ]}
          nowMs={0}
          onOpenDispatch={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      </NextIntlClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear' })).toBeDisabled()
  })
})
