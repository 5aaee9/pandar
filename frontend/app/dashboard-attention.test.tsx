import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { NextIntlClientProvider } from 'next-intl'
import { describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { computeAttention, computeHealth, statusMeta } from './dashboard-attention'
import type { Job } from './dashboard-types'
import { NeedsAttention } from './needs-attention'

function job({
  status = 'failed',
  error = null,
  printStatus = 'pending',
  printError = null,
  commandStatus = 'failed',
}: {
  status?: Job['status']
  error?: string | null
  printStatus?: Job['print']['status']
  printError?: string | null
  commandStatus?: Job['command']['status']
} = {}): Job {
  return {
    id: 'job-1',
    tenant_id: 'tenant-1',
    printer_id: 'printer-1',
    agent_id: 'agent-1',
    artifact_id: 'artifact-1',
    command_id: 'command-1',
    status,
    error,
    created_at: '2026-07-15T00:00:00Z',
    updated_at: '2026-07-15T00:00:00Z',
    print: {
      status: printStatus,
      printer_state: null,
      progress_percent: null,
      remaining_time_minutes: null,
      current_layer: null,
      total_layers: null,
      active_file: null,
      last_progress_percent: null,
      last_layer: null,
      error: printError,
      started_at: null,
      finished_at: null,
      updated_at: null,
    },
    command: {
      id: 'command-1',
      kind: 'print_project_file',
      status: commandStatus,
    },
    artifact: {
      id: 'artifact-1',
      tenant_id: 'tenant-1',
      filename: 'Untitled.gcode.3mf',
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
  }
}

function attentionFor(jobValue: Job) {
  return computeAttention({
    agents: [],
    printers: [],
    jobs: [jobValue],
    nowMs: 0,
  })
}

describe('job failure attention', () => {
  it('renders the complete dispatch failure reason returned by the API', () => {
    const reason =
      'dispatch print job job-1: flow calibration is not supported for model N6'
    const items = attentionFor(job({ error: reason }))

    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <NeedsAttention items={items} onOpenReprint={vi.fn()} selectedTenant={null} />
      </NextIntlClientProvider>,
    )

    expect(screen.getByText(`Reason: ${reason}`)).toBeInTheDocument()
  })

  it('opens the configurable Reprint dialog for a physical print failure', async () => {
    const onOpenReprint = vi.fn()
    const items = attentionFor(
      job({ status: 'succeeded', commandStatus: 'succeeded', printStatus: 'failed' }),
    )

    render(
      <NextIntlClientProvider locale="en" messages={en}>
        <NeedsAttention
          items={items}
          onOpenReprint={onOpenReprint}
          selectedTenant={{
            id: 'tenant-1',
            slug: 'dev',
            display_name: 'Dev Tenant',
            created_at: '2026-07-15T00:00:00Z',
          }}
        />
      </NextIntlClientProvider>,
    )

    await userEvent.click(screen.getByRole('button', { name: 'Reprint' }))

    expect(onOpenReprint).toHaveBeenCalledWith('job-1')
  })

  it('uses the physical print error for a physical failure', () => {
    const [item] = attentionFor(
      job({
        status: 'succeeded',
        error: 'dispatch error',
        printStatus: 'failed',
        printError: 'nozzle temperature fault',
      }),
    )

    expect(item.detailKey?.values).toEqual({ reason: 'nozzle temperature fault' })
  })

  it('treats persisted stalled jobs as recoverable warnings, not active failures', () => {
    const stalled = job({
      status: 'succeeded',
      commandStatus: 'succeeded',
      printStatus: 'stalled',
    })
    const [item] = attentionFor(stalled)

    expect(item).toMatchObject({
      severity: 'warning',
      reason: 'job_stalled',
      title: 'Job stalled',
      label: 'Untitled.gcode.3mf · did not start within 15 minutes',
      ageMs: null,
    })
    expect(computeHealth([], [], [stalled])).toMatchObject({
      jobsActive: 0,
      jobsFailed: 0,
    })
    expect(statusMeta('stalled')).toEqual({
      severity: 'warning',
      label: 'Stalled',
    })
  })

  it('does not infer stalled from a pending job and the browser clock', () => {
    expect(
      attentionFor(
        job({ status: 'succeeded', commandStatus: 'succeeded' }),
      ),
    ).toEqual([])
  })

  it('omits the reason row when the API has no failure cause', () => {
    const [item] = attentionFor(job())

    expect(item.detailKey).toBeUndefined()
  })
})
