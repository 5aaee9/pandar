import { useState } from 'react'
import { NextIntlClientProvider } from 'next-intl'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { JobHistory } from './dashboard-job-history'
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

function JobFilterRouteHarness() {
  const [showJobs, setShowJobs] = useState(true)
  const first = job()
  const second = job({
    id: 'job-2',
    artifact_id: 'artifact-2',
    command_id: 'command-2',
    command: { ...first.command, id: 'command-2' },
    artifact: {
      ...first.artifact,
      id: 'artifact-2',
      filename: 'cube.3mf',
    },
  })

  return (
    <>
      <button onClick={() => setShowJobs((current) => !current)} type="button">
        Switch route
      </button>
      {showJobs ? (
        <JobHistory
          agents={[]}
          jobs={[first, second]}
          nowMs={0}
          onOpenDispatch={vi.fn()}
          onOpenReprint={vi.fn()}
          printers={[]}
          selectedTenant={tenant}
        />
      ) : (
        <div>Devices route</div>
      )}
    </>
  )
}

function renderFilterRouteHarness() {
  return render(
    <QueryClientProvider client={createTestQueryClient()}>
      <NextIntlClientProvider locale="en" messages={en}>
        <JobFilterRouteHarness />
      </NextIntlClientProvider>
    </QueryClientProvider>,
  )
}

describe('JobHistory filters', () => {
  it('resets its typed filters after navigating away and back', async () => {
    const user = userEvent.setup()
    renderFilterRouteHarness()

    await user.type(screen.getByRole('searchbox'), 'benchy')
    expect(screen.getByText('benchy.3mf')).toBeInTheDocument()
    expect(screen.queryByText('cube.3mf')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Switch route' }))
    await user.click(screen.getByRole('button', { name: 'Switch route' }))

    expect(screen.getByText('benchy.3mf')).toBeInTheDocument()
    expect(screen.getByText('cube.3mf')).toBeInTheDocument()
  })
})

describe('JobHistory rows', () => {
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
