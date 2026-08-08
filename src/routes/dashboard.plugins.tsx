/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Archive, HardDrive, Loader2 } from "lucide-react"

import { getPlugins, type Plugin } from "@/lib/api"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"

export const Route = createFileRoute("/dashboard/plugins")({
  component: PluginsRoute,
})

function PluginsRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const [plugins, setPlugins] = React.useState<Plugin[] | null>(null)

  // The API describes plugins in English; these are fixed built-ins, so the
  // panel shows localized copy and falls back to the server text for anything
  // it doesn't know about yet.
  const LOCALIZED: Record<string, { name: string; description: string }> = {
    s3_storage: {
      name: t`S3 compatible storage`,
      description: t`Store uploaded media in an S3 bucket (AWS S3, Cloudflare R2, MinIO, DigitalOcean Spaces) instead of the local disk.`,
    },
    amazon_ses: {
      name: t`Amazon SES`,
      description: t`Send mail through Amazon SES — a notification when somebody fills in one of this site's forms.`,
    },
    video: {
      name: t`Video`,
      description: t`Host lesson videos on Bunny Stream or Cloudflare Stream. Files go straight from the browser to them, and every address expires.`,
    },
    backup: {
      name: t`Backups`,
      description: t`Take the database, and the uploaded files if you want them, into a single archive — on a schedule, to the disk or to your S3 bucket.`,
    },
  }

  const SETTINGS_PATH: Record<
    string,
    | "/dashboard/plugins/s3"
    | "/dashboard/plugins/backup"
    | "/dashboard/plugins/email"
    | "/dashboard/plugins/video"
  > = {
    s3_storage: "/dashboard/plugins/s3",
    video: "/dashboard/plugins/video",
    amazon_ses: "/dashboard/plugins/email",
    backup: "/dashboard/plugins/backup",
  }

  React.useEffect(() => {
    getPlugins()
      .then(setPlugins)
      .catch(() => setPlugins([]))
  }, [])

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Plugins`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Built-in integrations you can switch on and configure.`}
        </p>
      </div>

      {plugins === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {plugins.map((plugin) => (
            <Card key={plugin.id}>
              <CardContent className="flex flex-wrap items-start gap-x-4 gap-y-3 pt-6">
                <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-muted">
                  {plugin.id === "backup" ? (
                    <Archive className="size-5" />
                  ) : (
                    <HardDrive className="size-5" />
                  )}
                </span>
                <div className="min-w-0 flex-1 basis-48">
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="font-medium">
                      {LOCALIZED[plugin.id]?.name ?? plugin.name}
                    </p>
                    <Badge variant={plugin.enabled ? "default" : "secondary"}>
                      {plugin.enabled
                        ? t`Enabled`
                        : plugin.configured
                          ? t`Disabled`
                          : t`Not configured`}
                    </Badge>
                  </div>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {LOCALIZED[plugin.id]?.description ?? plugin.description}
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="ml-auto shrink-0"
                  onClick={() =>
                    navigate({
                      to: SETTINGS_PATH[plugin.id] ?? "/dashboard/plugins/s3",
                    })
                  }
                >
                  {t`Configure`}
                </Button>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </>
  )
}
