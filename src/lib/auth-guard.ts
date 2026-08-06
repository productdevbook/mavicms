import { redirect } from "@tanstack/react-router"

import { getCurrentUser, type CurrentUser } from "@/lib/api"

export async function requireAuth(currentHref: string): Promise<{
  user: CurrentUser
}> {
  const user = await getCurrentUser().catch(() => {
    throw redirect({ to: "/login", search: { redirect: currentHref } })
  })
  return { user }
}
