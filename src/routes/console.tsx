/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"

import { getConsoleAccount } from "@/lib/api"
import { ConsoleShell } from "@/components/console/console-shell"

export const Route = createFileRoute("/console")({
  // Asked for once here rather than on every page below, which each used to
  // fetch it and each used to redirect on its own.
  beforeLoad: async () => {
    const account = await getConsoleAccount().catch(() => {
      throw redirect({ to: "/console/login" })
    })
    return { account }
  },
  component: ConsoleLayout,
})

function ConsoleLayout() {
  const { account } = Route.useRouteContext()

  return (
    <ConsoleShell account={account}>
      <Outlet />
    </ConsoleShell>
  )
}
