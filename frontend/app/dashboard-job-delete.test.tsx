import { NextIntlClientProvider } from 'next-intl'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { JobHistory } from './dashboard-job-history'
import { isRetryDispatchSafe } from './dashboard-job-status'
import type { Job, Tenant } from './dashboard-types'
import { JobsView } from './dashboard-view-content'

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
    tenant_id: tenant.id,
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
      ams_mapping_info: null,
      filament_usage: [],
    },
    ...overrides,
  }
}

function renderHistory({
  jobs = [job()],
  canManageJobs = true,
  onDeleteRedirect = vi.fn(),
  onOpenReprint = vi.fn(),
}: {
  jobs?: Job[]
  canManageJobs?: boolean
  onDeleteRedirect?: (url: string) => void
  onOpenReprint?: (job: Job) => void
} = {}) {
  return {
    onDeleteRedirect,
    onOpenReprint,
    ...render(
      <QueryClientProvider client={createTestQueryClient()}>
        <NextIntlClientProvider locale="en" messages={en}>
          <JobHistory
            canManageJobs={canManageJobs}
            agents={[]}
            jobs={jobs}
            nowMs={Date.parse('2026-07-15T01:00:00Z')}
            onDeleteRedirect={onDeleteRedirect}
            onOpenDispatch={vi.fn()}
            onOpenReprint={onOpenReprint}
            printers={[]}
            selectedTenant={tenant}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    ),
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('JobHistory row actions', () => {
  it('mirrors every backend dispatch retry safety predicate', () => {
    const safe = job({
      status: 'failed',
      print: {
        ...job().print,
        status: 'pending',
        progress_percent: 0,
        current_layer: 0,
        started_at: null,
      },
      command: {
        id: 'command-1',
        kind: 'other',
        status: 'failed',
      },
    })

    expect(isRetryDispatchSafe(safe)).toBe(true)
    for (const unsafe of ([
      { ...safe, status: 'queued' },
      { ...safe, command: { ...safe.command, status: 'succeeded' } },
      { ...safe, print: { ...safe.print, status: 'running' } },
      {
        ...safe,
        print: { ...safe.print, started_at: '2026-07-15T00:00:00Z' },
      },
      { ...safe, print: { ...safe.print, progress_percent: 1 } },
      { ...safe, print: { ...safe.print, current_layer: 1 } },
    ] satisfies Job[])) {
      expect(isRetryDispatchSafe(unsafe)).toBe(false)
    }
  })

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

  it('opens the print dialog with the terminal job selected for Reprint', async () => {
    const user = userEvent.setup()
    const { onOpenReprint } = renderHistory()

    const reprint = screen.getByRole('button', {
      name: 'Reprint benchy.3mf',
    })
    expect(reprint).toBeEnabled()
    expect(reprint.closest('form')).toBeNull()

    await user.click(reprint)

    expect(onOpenReprint).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'job-1', artifact_id: 'artifact-1' }),
    )
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

  it('offers dispatch retry when dispatch failed before printing started', () => {
    renderHistory({
      jobs: [
        job({
          status: 'failed',
          print: {
            ...job().print,
            status: 'pending',
            progress_percent: 0,
            current_layer: 0,
            started_at: null,
            finished_at: null,
            updated_at: null,
          },
          command: {
            id: 'command-1',
            kind: 'print_project_file',
            status: 'failed',
          },
        }),
      ],
    })

    const retry = screen.getByRole('button', {
      name: 'Retry dispatch benchy.3mf',
    })
    expect(retry).toBeEnabled()
    const form = retry.closest('form')
    if (!form) throw new Error('retry action must submit a form')
    const formData = new FormData(form)
    expect(formData.get('tenant_id')).toBe('tenant-1')
    expect(formData.get('job_id')).toBe('job-1')
    expect(formData.get('return_to')).toBe('jobs')
    expect(
      screen.queryByRole('button', { name: 'Reprint benchy.3mf' }),
    ).not.toBeInTheDocument()
  })

  it('hides dispatch retry after physical print evidence appears', () => {
    renderHistory({
      jobs: [
        job({
          status: 'failed',
          print: {
            ...job().print,
            status: 'pending',
            progress_percent: 1,
            current_layer: 0,
            started_at: null,
            finished_at: null,
          },
          command: {
            id: 'command-1',
            kind: 'print_project_file',
            status: 'failed',
          },
        }),
      ],
    })

    expect(
      screen.queryByRole('button', { name: 'Retry dispatch benchy.3mf' }),
    ).not.toBeInTheDocument()
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
      '/jobs?status=job_deleted',
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
      <QueryClientProvider client={createTestQueryClient()}>
        <NextIntlClientProvider locale="en" messages={en}>
          <JobsView
            selectedTenant={tenant}
            printers={[]}
            agents={[]}
            jobs={[job()]}
            nowMs={Date.parse('2026-07-15T01:00:00Z')}
            canManageJobs={true}
          />
        </NextIntlClientProvider>
      </QueryClientProvider>,
    )

    expect(screen.getByRole('button', { name: 'Clear jobs' })).toBeEnabled()
    expect(
      screen.getByRole('button', { name: 'Delete benchy.3mf' }),
    ).toBeEnabled()
  })
})
