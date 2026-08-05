import { redirect } from "@tanstack/react-router"

import { getCurrentUser } from "@/lib/api"

export async function requireAuth(currentHref: string) {
  await getCurrentUser().catch(() => {
    throw redirect({ to: "/login", search: { redirect: currentHref } })
  })
}
