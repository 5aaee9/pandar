import { redirect } from 'next/navigation'

import { firstParam, type DashboardPageProps } from './dashboard-data'
import { dashboardRootRedirectTarget } from './dashboard-shell'

export default async function Page({ searchParams }: DashboardPageProps) {
  const params = await searchParams
  redirect(
    dashboardRootRedirectTarget({
      tenant: firstParam(params?.tenant),
      command: firstParam(params?.command),
      status: firstParam(params?.status),
    }),
  )
}
