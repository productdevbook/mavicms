import { redirect } from "@tanstack/react-router"

import { getCurrentUser, getSetupStatus, type CurrentUser } from "@/lib/api"

export async function requireAuth(currentHref: string): Promise<{
  user: CurrentUser
  /** What this installation calls itself, for the tab and the header. */
  site: string | null
}> {
  const user = await getCurrentUser().catch(() => {
    throw redirect({ to: "/login", search: { redirect: currentHref } })
  })
  // Already public and already cached by the sign-in page before this.
  const status = await getSetupStatus().catch(() => null)

  return { user, site: status?.site_title ?? null }
}
