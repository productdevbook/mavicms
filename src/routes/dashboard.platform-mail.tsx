/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Mails } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getPlatformMail,
  savePlatformMail,
  setSiteMailAllowance,
  type PlatformMail,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"

export const Route = createFileRoute("/dashboard/platform-mail")({
  component: PlatformMailRoute,
})

function PlatformMailRoute() {
  const { t } = useLingui()

  const [held, setHeld] = React.useState<PlatformMail | null>(null)
  const [region, setRegion] = React.useState("")
  const [keyId, setKeyId] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [from, setFrom] = React.useState("")
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    getPlatformMail()
      .then((mail) => {
        setHeld(mail)
        setRegion(mail.region)
        setFrom(mail.from_address)
      })
      .catch(() => setHeld(null))
  }, [])

  React.useEffect(load, [load])

  const save = async () => {
    setBusy(true)
    try {
      await savePlatformMail({
        region: region.trim(),
        access_key_id: keyId.trim(),
        // Left out keeps what is stored, so saving the form after reading it
        // does not blank the one field it was never shown.
        ...(secret ? { secret_access_key: secret } : {}),
        from_address: from.trim(),
      })
      setSecret("")
      load()
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    } finally {
      setBusy(false)
    }
  }

  const change = async (siteId: string, sends: string, a_day?: number) => {
    try {
      await setSiteMailAllowance(siteId, { sends, a_day })
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not change it`
      )
    }
  }

  const handedOut = (held?.sites ?? [])
    .filter((site) => site.sends === "shared")
    .reduce((sum, site) => sum + site.a_day, 0)

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Mail`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`One Amazon account, lent to the sites that have none of their own. Each site sends as its own Amazon tenant, so one site's complaints do not become everybody's.`}
        </p>
      </div>

      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-4 rounded-xl border border-border p-4">
          <h2 className="text-base font-semibold">{t`The account`}</h2>

          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="pm-region">{t`Region`}</Label>
              <Input
                id="pm-region"
                value={region}
                onChange={(event) => setRegion(event.target.value)}
                placeholder="eu-central-1"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="pm-from">{t`Send as`}</Label>
              <Input
                id="pm-from"
                value={from}
                onChange={(event) => setFrom(event.target.value)}
                placeholder="posta@example.com"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="pm-key">{t`Access key`}</Label>
              <Input
                id="pm-key"
                value={keyId}
                onChange={(event) => setKeyId(event.target.value)}
                placeholder="AKIA…"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="pm-secret">{t`Secret`}</Label>
              <Input
                id="pm-secret"
                type="password"
                autoComplete="off"
                value={secret}
                onChange={(event) => setSecret(event.target.value)}
                placeholder={
                  held?.has_secret_access_key
                    ? t`Stored — type to replace`
                    : t`Not set`
                }
              />
            </div>
          </div>

          <p className="text-sm text-muted-foreground">
            {t`The exact permissions this needs are in docs/shared-email.md. A new Amazon account may only send 200 messages a day and only to addresses it has verified, which is no use for lending — ask Amazon for production access before turning any site on.`}
          </p>

          <div>
            <Button onClick={() => void save()} disabled={busy}>
              {busy ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
          </div>

          {held?.configured ? (
            <div className="flex flex-wrap gap-x-6 gap-y-1 border-t border-border pt-3 text-sm text-muted-foreground">
              <span>
                {t`Amazon allows this account ${held.account_a_second ?? 0} a second`}
              </span>
              <span>
                {t`Sent in the last day: ${Math.round(held.account_last_day ?? 0)}`}
              </span>
              <span>{t`Handed out to sites: ${handedOut} a day`}</span>
            </div>
          ) : null}
        </div>

        <div className="flex flex-col gap-3">
          <h2 className="text-base font-semibold">{t`Sites`}</h2>

          {!held ? (
            <div className="flex justify-center py-8">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : held.sites.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Mails />
                </EmptyMedia>
                <EmptyTitle>{t`No site is using the server's account`}</EmptyTitle>
                <EmptyDescription>
                  {t`A site appears here once it has been lent the account. Until then it sends with its own settings, or not at all.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
              {held.sites.map((site) => (
                <div
                  key={site.site_id}
                  className="flex flex-wrap items-center gap-3 px-4 py-3"
                >
                  <div className="min-w-40 flex-1">
                    <p className="truncate text-sm font-medium">{site.host}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {site.sends === "shared"
                        ? t`${site.sent_today} of ${site.a_day} used today`
                        : t`Sends with its own account`}
                      {site.amazon_says ? ` · ${site.amazon_says}` : ""}
                    </p>
                  </div>

                  {site.sends === "shared" ? (
                    <>
                      <Input
                        className="w-28"
                        type="number"
                        min={0}
                        defaultValue={String(site.a_day)}
                        aria-label={t`Messages a day`}
                        onBlur={(event) => {
                          const wanted = Number(event.target.value)
                          if (wanted !== site.a_day) {
                            void change(site.site_id, "shared", wanted)
                          }
                        }}
                      />
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => void change(site.site_id, "own")}
                      >
                        {t`Stop lending`}
                      </Button>
                    </>
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void change(site.site_id, "shared", 200)}
                    >
                      {t`Lend the account`}
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}

          <p className="text-sm text-muted-foreground">
            {t`A site starts at 200 a day — a contact form and some notifications, not a mailing list. Nought stops it without taking anything away: the lists and the history stay, and raising the number starts it again.`}
          </p>
        </div>
      </div>
    </>
  )
}
