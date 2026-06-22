import { cookies } from 'next/headers'

const apiToken = process.env.APP_API_TOKEN
const staticAuthToken = process.env.APP_AUTH_BEARER_TOKEN
const authCookieName = process.env.APP_AUTH_COOKIE_NAME ?? 'pandar_auth_token'

export async function apiHeaders(contentType?: string) {
  const headers: Record<string, string> = {}
  if (contentType) {
    headers['content-type'] = contentType
  }

  const cookieStore = await cookies()
  const cookieToken = cookieStore.get(authCookieName)?.value
  const token = cookieToken || staticAuthToken || apiToken
  if (token) {
    headers.authorization = `Bearer ${token}`
  }

  return Object.keys(headers).length > 0 ? headers : undefined
}
