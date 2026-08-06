'use client'

import { unstable_rethrow } from 'next/navigation'
import { Component, type ReactNode } from 'react'

import { Button } from '@/components/ui/button'

interface Props {
  children: ReactNode
  fallback?: ReactNode
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void
}

interface State {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    unstable_rethrow(error)
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    this.props.onError?.(error, errorInfo)
    console.error('ErrorBoundary caught an error:', error, errorInfo)
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback
      }
      return (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
          <h3 className="font-semibold">Something went wrong</h3>
          <p className="mt-1">{this.state.error?.message ?? 'An unexpected error occurred'}</p>
          <Button
            className="mt-3 bg-destructive text-destructive-foreground hover:bg-destructive/90"
            onClick={() => this.setState({ hasError: false, error: null })}
            size="sm"
          >
            Try again
          </Button>
        </div>
      )
    }

    return this.props.children
  }
}
