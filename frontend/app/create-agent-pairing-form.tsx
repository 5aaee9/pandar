'use client'

import { useState } from 'react'

import { CreateAgentPairingFormInner } from './create-agent-pairing-form-inner'

export function CreateAgentPairingForm({ tenantId }: { tenantId: string }) {
  const [nonce, setNonce] = useState(0)
  return (
    <CreateAgentPairingFormInner
      key={nonce}
      onCreateAnother={() => setNonce((value) => value + 1)}
      tenantId={tenantId}
    />
  )
}
