import { NextIntlClientProvider } from 'next-intl'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { JobHistory } from './dashboard-job-history'
import { useDashboardFilterStore } from './dashboard-filter-store'
import type { Job, Tenant } from './dashboard-types'

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })
}

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
      <QueryClientProvider client={createTestQueryClient()}>
        <NextIntlClientProvider locale="en" messages={en}>
          <JobHistory
            agents={[]}
            jobs={jobs}
            nowMs={nowMs}
            onClearRedirect={onClearRedirect}
            onOpenDispatch={vi.fn()}
            onOpenReprint={vi.fn()}
            printers={[]}
            selectedTenant={selectedTenant}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    ),
  }
}

describe('JobHistory actions', () => {
  beforeEach(() => {
    useDashboardFilterStore.getState().reset()
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  it('renders one list item per job without nested list items', () => {
    const { container } = renderHistory()
    const list = container.querySelector('ul[aria-label="Print jobs"]')

    expect(list?.querySelectorAll(':scope > li')).toHaveLength(1)
    expect(list?.querySelector('li li')).toBeNull()
  })

  it('confirms clear, deletes through the tenant proxy, and redirects on success', async () => {
    const user = userEvent.setup()
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    const { onClearRedirect } = renderHistory()

    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))

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

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeDisabled()

    rerender(
      <QueryClientProvider client={createTestQueryClient()}>
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
            onOpenReprint={vi.fn()}
            printers={[]}
            selectedTenant={tenant}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeDisabled()
  })

  it('keeps the confirmation open and reports a failed clear request', async () => {
    const user = userEvent.setup()
    const onClearRedirect = vi.fn()
    vi.stubGlobal('fetch', vi.fn(async () => new Response(null, { status: 403 })))
    renderHistory({ onClearRedirect })

    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))
    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))

    expect(
      await screen.findByText("You don't have permission to clear jobs."),
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
        status: 'stalled',
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
        status: 'pending',
        updated_at: '2026-07-15T00:10:00Z',
      },
    })
    renderHistory({
      jobs: [job({ id: 'job-terminal' }), stalled, running, recentlyUpdated],
      nowMs: Date.parse('2026-07-15T00:16:00Z'),
    })

    await user.click(screen.getByRole('button', { name: 'Clear jobs' }))

    expect(
      screen.getByText(
        'This removes 2 terminal or stalled waiting jobs. Running and other active jobs are kept.',
      ),
    ).toBeVisible()
  })

  it('trusts the persisted stalled status instead of the browser clock', () => {
    const pending = job({
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
    const { rerender } = renderHistory({
      jobs: [pending],
      nowMs: Date.parse('2026-07-15T12:00:00Z'),
    })

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeDisabled()

    rerender(
      <QueryClientProvider client={createTestQueryClient()}>
        <NextIntlClientProvider locale="en" messages={en}>
          <JobHistory
            agents={[]}
            jobs={[
              job({
                ...pending,
                print: { ...pending.print, status: 'stalled' },
              }),
            ]}
            nowMs={Date.parse('2026-07-15T00:00:00Z')}
            onOpenDispatch={vi.fn()}
            onOpenReprint={vi.fn()}
            printers={[]}
            selectedTenant={tenant}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeEnabled()
  })

  it('excludes persisted stalled jobs from the active filter', async () => {
    const user = userEvent.setup()
    renderHistory({
      jobs: [
        job({
          id: 'job-stalled',
          artifact: { ...job().artifact, filename: 'stalled.3mf' },
          print: { ...job().print, status: 'stalled' },
        }),
        job({
          id: 'job-running',
          artifact: { ...job().artifact, filename: 'running.3mf' },
          print: { ...job().print, status: 'running' },
        }),
      ],
    })

    await user.selectOptions(screen.getByLabelText('Filter by status'), 'active')

    expect(screen.queryByText('stalled.3mf')).not.toBeInTheDocument()
    expect(screen.getByText('running.3mf')).toBeVisible()
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

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeEnabled()

    rerender(
      <QueryClientProvider client={createTestQueryClient()}>
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
            onOpenReprint={vi.fn()}
            printers={[]}
            selectedTenant={tenant}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeDisabled()
  })
})

describe('JobHistory rows', () => {
  beforeEach(() => {
    useDashboardFilterStore.getState().reset()
  })

  it('replaces empty print metrics with the recovery state', () => {
    renderHistory({
      jobs: [
        job({
          print: {
            ...job().print,
            status: 'pending',
            progress_percent: null,
            remaining_time_minutes: null,
            current_layer: null,
            total_layers: 100,
            last_progress_percent: null,
            last_layer: null,
            started_at: null,
            finished_at: null,
            updated_at: null,
          },
        }),
      ],
    })

    expect(screen.getByText('Waiting for the print to start')).toBeVisible()
    expect(screen.queryByText('Layers -')).not.toBeInTheDocument()
    expect(screen.queryByText('Layers -/100')).not.toBeInTheDocument()
    expect(screen.queryByText('Remaining -')).not.toBeInTheDocument()
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
  })

  it('keeps zero-valued running metrics visible', () => {
    renderHistory({
      jobs: [
        job({
          print: {
            ...job().print,
            status: 'running',
            progress_percent: 0,
            remaining_time_minutes: 75,
            current_layer: 0,
            total_layers: 100,
            finished_at: null,
          },
        }),
      ],
    })

    expect(screen.getByText('Printing now')).toBeVisible()
    expect(screen.getByText('0%')).toBeVisible()
    expect(screen.getByText('Layers 0/100')).toBeVisible()
    expect(screen.getByText('Remaining 1h 15m')).toBeVisible()
    expect(screen.getByRole('progressbar', { name: 'Print progress' })).toHaveAttribute(
      'aria-valuenow',
      '0',
    )
  })

  it('keeps structured job details collapsed until requested', async () => {
    const user = userEvent.setup()
    renderHistory()
    const summary = screen.getByText('Details')
    const details = summary.closest('details') as HTMLDetailsElement

    expect(details.open).toBe(false)
    expect(screen.getByText('Project:')).not.toBeVisible()

    await user.click(summary)

    expect(details.open).toBe(true)
    expect(screen.getByText('Project:')).toBeVisible()
    expect(screen.getByText('job-1')).toBeVisible()
  })
})
