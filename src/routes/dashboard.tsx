import { createFileRoute, Outlet } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { DashboardShell } from "@/components/dashboard/dashboard-shell"

export const Route = createFileRoute("/dashboard")({
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: () => (
    <DashboardShell>
      <Outlet />
    </DashboardShell>
  ),
})
