import { NextIntlClientProvider } from 'next-intl'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import en from '../messages/en.json'
import { AgentSettingsPanel } from './agent-settings-panel'
import { LinkedAgentsSection } from './diagnostics-panel'
import type { Agent, Tenant } from './dashboard-types'

vi.mock('./actions', () => ({
  deleteAgent: vi.fn(),
  discoverPrinters: vi.fn(),
  refreshPrinters: vi.fn(),
}))

const tenant: Tenant = {
  id: 'tenant-1',
  slug: 'tenant-one',
  display_name: 'Tenant One',
  created_at: '2026-01-01T00:00:00Z',
}

const agent: Agent = {
  id: 'agent-1',
  tenant_id: tenant.id,
  name: 'Agent One',
  status: 'offline',
  created_at: '2026-01-02T00:00:00Z',
}

function renderWithMessages(children: React.ReactNode) {
  return render(
    <NextIntlClientProvider locale="en" messages={en}>
      {children}
    </NextIntlClientProvider>,
  )
}

describe('agent settings navigation', () => {
  it('moves discovery controls out of the linked agents table and adds settings', () => {
    renderWithMessages(<LinkedAgentsSection agents={[agent]} selectedTenant={tenant} />)

    expect(screen.queryByRole('spinbutton')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Discover printers for Agent One' })).not.toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Settings for Agent One' })).toHaveAttribute(
      'href',
      '/agents/agent-1/settings?tenant=tenant-1',
    )
  })

  it('renders discovery and timeout controls on the dedicated settings page', () => {
    const { container } = renderWithMessages(
      <AgentSettingsPanel agent={agent} selectedTenant={tenant} />,
    )

    expect(screen.getByRole('heading', { name: 'Agent One settings' })).toBeVisible()
    expect(screen.getByRole('spinbutton', { name: 'Timeout (seconds)' })).toHaveValue(5)
    expect(screen.getByRole('button', { name: 'Discover printers for Agent One' })).toBeVisible()
    expect(container.querySelector('input[name="return_to"]')).toHaveValue('agent_settings')
  })
})
