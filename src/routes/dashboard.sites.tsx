/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Building2, Globe, Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createSite,
  deleteSite,
  getAgencies,
  getSites,
  updateAgency,
  updateSite,
  type Agency,
  type Site,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
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
  const [agencies, setAgencies] = React.useState<Agency[]>([])
  const [adding, setAdding] = React.useState(false)
  const [host, setHost] = React.useState("")
  const [databaseUrl, setDatabaseUrl] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  // Deleting asks for the address back: an id in a URL is not something
  // anybody reads before clicking.
  const [removing, setRemoving] = React.useState<Site | null>(null)
  const [typed, setTyped] = React.useState("")

  const load = React.useCallback(() => {
    getSites()
      .then(setSites)
      .catch(() => toast.error(t`Could not load the sites`))
    getAgencies()
      .then(setAgencies)
      .catch(() => undefined)
  }, [t])

  React.useEffect(load, [load])

  const add = async () => {
    setSaving(true)
    try {
      await createSite(host.trim(), databaseUrl.trim())
      setAdding(false)
      setHost("")
      setDatabaseUrl("")
      load()
      toast.success(t`Site added`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add the site`
      )
    } finally {
      setSaving(false)
    }
  }

  const setActive = async (site: Site, active: boolean) => {
    try {
      await updateSite(site.id, { active })
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not change it`
      )
    }
  }

  const remove = async () => {
    if (!removing) return
    setSaving(true)
    try {
      await deleteSite(removing.id, typed.trim())
      setRemoving(null)
      setTyped("")
      load()
      toast.success(t`Site removed`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not remove it`
      )
    } finally {
      setSaving(false)
    }
  }

  const setLimit = async (agency: Agency, site_limit: number) => {
    try {
      await updateAgency(agency.id, { site_limit })
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not change it`
      )
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
            {t`Other sites this server hosts. Each gets its own database schema, accounts and uploads.`}
          </p>
        </div>
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`Add site`}
        </Button>
      </div>

      <div className="flex max-w-3xl flex-col gap-8">
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
                    href={`https://${site.host}/admin`}
                    target="_blank"
                    rel="noreferrer"
                    className="truncate text-sm font-medium hover:underline"
                  >
                    {site.host}
                  </a>
                  <p className="truncate text-xs text-muted-foreground">
                    {site.database_url
                      ? t`Its own database server`
                      : t`Schema ${site.schema}`}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    {site.active ? t`On` : t`Off`}
                  </span>
                  <Switch
                    checked={site.active}
                    onCheckedChange={(next) => void setActive(site, next)}
                  />
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  onClick={() => setRemoving(site)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}

        {agencies.length > 0 && (
          <div>
            <h2 className="mb-2 text-sm font-medium">{t`Agencies`}</h2>
            <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
              {agencies.map((agency) => (
                <div
                  key={agency.id}
                  className="flex items-center gap-3 px-4 py-3"
                >
                  <Building2 className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{agency.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {agency.email} · {t`${agency.sites} sites`}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <Label
                      htmlFor={`limit-${agency.id}`}
                      className="text-xs text-muted-foreground"
                    >
                      {t`Limit`}
                    </Label>
                    <Input
                      id={`limit-${agency.id}`}
                      type="number"
                      min={0}
                      max={9999}
                      defaultValue={agency.site_limit}
                      className="w-20"
                      onBlur={(event) => {
                        const next = Number(event.target.value)
                        if (next !== agency.site_limit) {
                          void setLimit(agency, Math.max(0, next))
                        }
                      }}
                    />
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">
                      {agency.active ? t`On` : t`Off`}
                    </span>
                    <Switch
                      checked={agency.active}
                      onCheckedChange={(active) => {
                        void updateAgency(agency.id, { active }).then(load)
                      }}
                    />
                  </div>
                </div>
              ))}
            </div>
            <p className="mt-2 text-sm text-muted-foreground">
              {t`Switching an agency off stops it signing in. Its sites keep serving — the customers are not the ones who fell out with anybody.`}
            </p>
          </div>
        )}
      </div>

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
                placeholder={t`Leave empty to use this server's database`}
              />
              <p className="text-sm text-muted-foreground">
                {t`The site gets a schema of its own on this server's database. One that outgrows it can be given an address of its own here.`}
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

      <Dialog
        open={removing !== null}
        onOpenChange={(open) => {
          if (!open) {
            setRemoving(null)
            setTyped("")
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Remove this site?`}</DialogTitle>
            <DialogDescription>
              {t`Its posts, its accounts, its uploads and its database schema all go. There is no undo. Type the address to confirm.`}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <Label htmlFor="confirm-host">{removing?.host}</Label>
            <Input
              id="confirm-host"
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => {
                setRemoving(null)
                setTyped("")
              }}
            >
              {t`Cancel`}
            </Button>
            <Button
              variant="destructive"
              onClick={() => void remove()}
              disabled={saving || typed.trim() !== removing?.host}
            >
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Remove`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
