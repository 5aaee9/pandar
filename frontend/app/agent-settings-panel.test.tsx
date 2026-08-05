import { NextIntlClientProvider } from 'next-intl'
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import en from '../messages/en.json'
import { AgentSettingsPanel } from './agent-settings-panel'
import type { Agent, Tenant } from './dashboard-types'

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

describe('agent settings panel', () => {
  it('renders agent connection details without discovery controls', () => {
    renderWithMessages(
      <AgentSettingsPanel agent={agent} />,
    )

    expect(
      screen.getByRole('heading', { name: 'Agent One settings' }),
    ).toBeVisible()
    expect(screen.getByText('agent-1')).toBeVisible()
    expect(screen.getByRole('link', { name: '← Back to agents' })).toHaveAttribute(
      'href',
      '/agents',
    )
    expect(screen.queryByRole('spinbutton')).not.toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: /Discover/ }),
    ).not.toBeInTheDocument()
  })
})
