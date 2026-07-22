'use client'

import { useQueryErrorResetBoundary } from '@tanstack/react-query'
import { ErrorBoundary } from './error-boundary'
import type { ReactNode } from 'react'

export function QueryErrorBoundary({ children }: { children: ReactNode }) {
  const { reset } = useQueryErrorResetBoundary()
  return (
    <ErrorBoundary
      onError={(error) => {
        console.error('Query error:', error)
      }}
      fallback={
        <div className="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
          <h3 className="font-semibold">Failed to load data</h3>
          <p className="mt-1">An error occurred while fetching data. Please try again.</p>
          <button
            className="mt-3 rounded-md bg-destructive px-3 py-1.5 text-xs font-medium text-destructive-foreground transition hover:bg-destructive/90"
            onClick={reset}
          >
            Retry
          </button>
        </div>
      }
    >
      {children}
    </ErrorBoundary>
  )
}
