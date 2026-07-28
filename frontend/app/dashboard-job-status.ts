import type { Job } from './dashboard-types'

const TERMINAL_JOB_STATUSES = new Set([
  'stalled',
  'completed',
  'failed',
  'cancelled',
])
const CLEARABLE_JOB_STATUSES = new Set(['succeeded', 'failed'])
const CLEARABLE_PRINT_STATUSES = new Set([
  'stalled',
  'completed',
  'failed',
  'cancelled',
])

export function isRetryDispatchSafe(job: Job): boolean {
  return (
    job.status.toLowerCase() === 'failed' &&
    job.command.status.toLowerCase() === 'failed' &&
    job.print.status.toLowerCase() === 'pending' &&
    job.print.started_at === null &&
    (job.print.progress_percent ?? 0) === 0 &&
    (job.print.current_layer ?? 0) === 0
  )
}

export function isClearableJob(job: Job): boolean {
  const status = job.status.toLowerCase()
  const commandStatus = job.command.status.toLowerCase()
  const printStatus = job.print.status.toLowerCase()
  if (
    !CLEARABLE_JOB_STATUSES.has(status) ||
    !CLEARABLE_JOB_STATUSES.has(commandStatus) ||
    job.command.kind !== 'print_project_file'
  ) {
    return false
  }
  if (CLEARABLE_PRINT_STATUSES.has(printStatus)) {
    return true
  }
  return (
    printStatus === 'pending' &&
    job.print.started_at === null &&
    (job.print.progress_percent ?? 0) === 0 &&
    (job.print.current_layer ?? 0) === 0 &&
    status === 'failed'
  )
}

export function jobMatchesStatus(job: Job, status: string): boolean {
  const dispatch = job.status.toLowerCase()
  const physical = job.print.status.toLowerCase()
  if (status === 'active') {
    return (
      !TERMINAL_JOB_STATUSES.has(dispatch) &&
      !TERMINAL_JOB_STATUSES.has(physical)
    )
  }
  if (status === 'failed') {
    return dispatch === 'failed' || physical === 'failed'
  }
  if (status === 'completed') {
    return dispatch === 'completed' || physical === 'completed'
  }
  return true
}
