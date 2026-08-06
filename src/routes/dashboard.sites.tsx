/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Globe, Loader2, Plus } from "lucide-react"
import { toast } from "sonner"

import { ApiError, createSite, getSites, type Site } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export const Route = createFileRoute("/dashboard/sites")({
  component: SitesRoute,
})

function SitesRoute() {
  const { t } = useLingui()
  const [sites, setSites] = React.useState<Site[] | null>(null)
  const [adding, setAdding] = React.useState(false)
  const [host, setHost] = React.useState("")
  const [databaseUrl, setDatabaseUrl] = React.useState("")
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    getSites()
      .then(setSites)
      .catch(() => toast.error(t`Could not load the sites`))
  }, [t])

  const add = async () => {
    setSaving(true)
    try {
      const site = await createSite(host.trim(), databaseUrl.trim())
      setSites((current) => [...(current ?? []), site])
      setAdding(false)
      setHost("")
      setDatabaseUrl("")
      toast.success(t`${site.host} is ready. Open it to set it up.`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add the site`
      )
    } finally {
      setSaving(false)
    }
  }

  if (!sites) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Sites`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Other sites this server hosts. Each keeps its own content, accounts and uploads.`}
          </p>
        </div>
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`Add site`}
        </Button>
      </div>

      {sites.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`No other sites yet`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {sites.map((site) => (
            <div key={site.id} className="flex items-center gap-3 px-4 py-3">
              <Globe className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <a
                  href={`https://${site.host}/dashboard`}
                  target="_blank"
                  rel="noreferrer"
                  className="truncate text-sm font-medium hover:underline"
                >
                  {site.host}
                </a>
                <p className="truncate text-xs text-muted-foreground">
                  {site.database_url
                    ? t`Its own database server`
                    : t`A database file of its own`}
                </p>
              </div>
              {!site.active && (
                <span className="rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                  {t`Switched off`}
                </span>
              )}
            </div>
          ))}
        </div>
      )}

      <Dialog open={adding} onOpenChange={setAdding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Add a site`}</DialogTitle>
            <DialogDescription>
              {t`Point the address at this server first — the site answers on whichever address you give it here.`}
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="host">{t`Address`}</Label>
              <Input
                id="host"
                value={host}
                onChange={(event) => setHost(event.target.value)}
                placeholder="example.com"
                autoFocus
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="database">{t`Database`}</Label>
              <Input
                id="database"
                value={databaseUrl}
                onChange={(event) => setDatabaseUrl(event.target.value)}
                placeholder={t`Leave empty for a file of its own`}
              />
              <p className="text-sm text-muted-foreground">
                {t`A file is enough for most sites. A busy one can be given a Postgres or MySQL address instead.`}
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void add()} disabled={!host.trim() || saving}>
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Add site`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
