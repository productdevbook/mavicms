import { createFileRoute, redirect } from "@tanstack/react-router"
import { Loader2 } from "lucide-react"

import { getSetupStatus } from "@/lib/api"

export const Route = createFileRoute("/")({
  loader: async () => {
    const status = await getSetupStatus().catch(() => ({
      database_configured: true,
      installed: true,
      site_title: null,
    }))
    throw redirect({ to: status.installed ? "/dashboard" : "/setup" })
  },
  pendingComponent: () => (
    <div className="flex min-h-svh items-center justify-center">
      <Loader2 className="size-6 animate-spin text-muted-foreground" />
    </div>
  ),
})
