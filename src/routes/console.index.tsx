/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ExternalLink, Globe, Loader2, Plus, Settings } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createConsoleSite,
  createSiteEntry,
  getConsoleSites,
  type ConsoleSite,
} from "@/lib/api"
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

export const Route = createFileRoute("/console/")({
  component: ConsoleRoute,
})

function ConsoleRoute() {
  const { t } = useLingui()
  const { account } = Route.useRouteContext()

  const [sites, setSites] = React.useState<ConsoleSite[] | null>(null)
  const [adding, setAdding] = React.useState(false)
  const [host, setHost] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [opening, setOpening] = React.useState<string | null>(null)

  React.useEffect(() => {
    getConsoleSites()
      .then(setSites)
      .catch(() => toast.error(t`Could not load your sites`))
  }, [t])

  const add = async () => {
    setSaving(true)
    try {
      const site = await createConsoleSite(host.trim())
      setSites((current) => [...(current ?? []), site])
      setAdding(false)
      setHost("")
      toast.success(t`${site.host} is ready`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add the site`
      )
    } finally {
      setSaving(false)
    }
  }

  // The link is good once and for two minutes, so it is fetched when the
  // button is pressed rather than sitting in the page waiting to be stale.
  const open = async (site: ConsoleSite) => {
    setOpening(site.id)
    try {
      const { url } = await createSiteEntry(site.id)
      window.open(url, "_blank", "noopener")
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not open the site`
      )
    } finally {
      setOpening(null)
    }
  }

  const atLimit = (sites?.length ?? 0) >= account.site_limit

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Your sites`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`${sites?.length ?? 0} of ${account.site_limit} used.`}
          </p>
        </div>
        <Button onClick={() => setAdding(true)} disabled={atLimit}>
          <Plus /> {t`Add site`}
        </Button>
      </div>

      {!sites ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : sites.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
          {t`No sites yet`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {sites.map((site) => (
            <div key={site.id} className="flex items-center gap-3 px-4 py-3">
              <Globe className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{site.host}</p>
                {!site.active && (
                  <p className="text-xs text-muted-foreground">
                    {t`Switched off`}
                  </p>
                )}
              </div>
              <Link
                to="/console/sites/$siteId"
                params={{ siteId: site.id }}
                className="flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <Settings className="size-4" />
                {t`Settings`}
              </Link>
              <Button
                variant="outline"
                size="sm"
                onClick={() => void open(site)}
                disabled={opening === site.id || !site.active}
              >
                {opening === site.id ? (
                  <Loader2 className="animate-spin" />
                ) : (
                  <ExternalLink />
                )}
                {t`Manage`}
              </Button>
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

          <div className="flex flex-col gap-2">
            <Label htmlFor="console-host">{t`Address`}</Label>
            <Input
              id="console-host"
              value={host}
              onChange={(event) => setHost(event.target.value)}
              placeholder="example.com"
              autoFocus
            />
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button
              onClick={() => void add()}
              disabled={!host.trim() || saving}
            >
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Add site`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
