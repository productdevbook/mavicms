/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"

import { getSetupStatus } from "@/lib/api"
import { SetupWizard } from "@/components/setup/setup-wizard"

export const Route = createFileRoute("/setup")({
  loader: async () => {
    const status = await getSetupStatus().catch(() => ({
      database_configured: false,
      installed: false,
      site_title: null,
    }))
    if (status.installed) {
      throw redirect({ to: "/dashboard" })
    }
    return status
  },
  component: SetupRoute,
})

function SetupRoute() {
  const navigate = useNavigate()
  const status = Route.useLoaderData()
  return (
    <SetupWizard
      initialDatabaseConfigured={status.database_configured}
      onComplete={() => navigate({ to: "/dashboard" })}
    />
  )
}
