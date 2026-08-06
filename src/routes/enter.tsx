/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, redirect } from "@tanstack/react-router"
import { Trans } from "@lingui/react/macro"

import { enterSite } from "@/lib/api"

/**
 * Where an agency lands when it opens one of its sites from the console.
 *
 * The token is spent here rather than in the console because a cookie can
 * only be set for the host the browser is talking to — and it is spent before
 * anything renders, so the address with the token in it is replaced by the
 * dashboard rather than left in history.
 */
export const Route = createFileRoute("/enter")({
  validateSearch: (search: Record<string, unknown>): { token?: string } => ({
    token: typeof search.token === "string" ? search.token : undefined,
  }),
  beforeLoad: async ({ search }) => {
    if (!search.token) throw redirect({ to: "/login" })
    await enterSite(search.token).catch(() => {
      throw redirect({ to: "/login" })
    })
    throw redirect({ to: "/dashboard" })
  },
  component: EnterRoute,
})

function EnterRoute() {
  return (
    <div className="flex min-h-svh items-center justify-center text-sm text-muted-foreground">
      <Trans>Signing you in…</Trans>
    </div>
  )
}
