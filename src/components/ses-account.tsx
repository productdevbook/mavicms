import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Copy,
  Loader2,
  Plus,
  Trash2,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  addSesIdentity,
  deleteSesIdentity,
  getSesAccount,
  getSesIdentities,
  getSesSuppressed,
  requestProductionAccess,
  setSesSending,
  setSesMailFrom,
  unsuppressSesAddress,
  type SesAccount,
  type SesIdentity,
  type SesSuppressed,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/**
 * The Amazon account behind a site's mail, managed from here rather than from
 * the AWS console.
 *
 * Every question people ask about SES is answered on this page: why a message
 * did not arrive (the account is in the sandbox, or the sender is not
 * verified), how much sending is left today, and which addresses Amazon will
 * refuse. Whoever runs a site should not need an AWS login to find out.
 *
 * Given a `siteId` it speaks for that site through the console; without one,
 * for the site whose panel this is.
 */
export function SesAccountPanel({
  siteId,
  ready,
}: {
  siteId?: string
  /** Whether there are settings to ask Amazon with. */
  ready: boolean
}) {
  const { t } = useLingui()

  const [account, setAccount] = React.useState<SesAccount | null>(null)
  const [identities, setIdentities] = React.useState<SesIdentity[]>([])
  const [suppressed, setSuppressed] = React.useState<SesSuppressed[]>([])
  const [problem, setProblem] = React.useState<string | null>(null)
  // Only loading when there is something to load; with no settings the
  // panel has nothing to ask and nothing to wait for.
  const [loading, setLoading] = React.useState(ready)
  const [busy, setBusy] = React.useState(false)
  const [newIdentity, setNewIdentity] = React.useState("")

  const [request, setRequest] = React.useState({
    mail_type: "TRANSACTIONAL",
    website_url: "",
    use_case_description: "",
  })

  // No setLoading(true) here: this runs from an effect on mount, where the
  // initial state already is loading, and from the handlers below, where the
  // button is what says something is happening.
  const load = React.useCallback(() => {
    if (!ready) {
      return
    }
    getSesAccount(siteId)
      .then((found) => {
        setAccount(found)
        setProblem(null)
        setRequest((current) => ({
          mail_type: found.mail_type || current.mail_type,
          website_url: found.website_url || current.website_url,
          use_case_description:
            found.use_case_description || current.use_case_description,
        }))
      })
      .catch((error) => {
        setAccount(null)
        setProblem(
          error instanceof ApiError ? error.message : t`Could not ask Amazon`
        )
      })
      .finally(() => setLoading(false))

    getSesIdentities(siteId).then(setIdentities).catch(() => setIdentities([]))
    getSesSuppressed(siteId).then(setSuppressed).catch(() => setSuppressed([]))
  }, [siteId, ready, t])

  React.useEffect(load, [load])

  const ask = async () => {
    setBusy(true)
    try {
      await requestProductionAccess(
        { ...request, contact_language: "EN", additional_contacts: [] },
        siteId
      )
      toast.success(t`Asked. Amazon usually answers within a day.`)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not ask`
      )
    } finally {
      setBusy(false)
    }
  }

  const add = async () => {
    setBusy(true)
    try {
      await addSesIdentity(newIdentity.trim(), siteId)
      setNewIdentity("")
      toast.success(t`Asked SES to verify it`)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add it`
      )
    } finally {
      setBusy(false)
    }
  }

  if (loading) {
    return (
      <div className="flex justify-center py-12">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (!ready) {
    return (
      <p className="rounded-xl border border-dashed border-border px-4 py-6 text-sm text-muted-foreground">
        {t`Fill in the region and the keys above and save them. Everything Amazon knows about this account then appears here: how much sending is left today, whether it is still in the sandbox, which senders are verified, and how the last two weeks went.`}
      </p>
    )
  }

  if (problem || !account) {
    return (
      <div className="flex flex-col gap-2 rounded-xl border border-dashed border-border px-4 py-6">
        <p className="text-sm text-muted-foreground">
          {problem ?? t`Could not ask Amazon`}
        </p>
        <p className="text-sm text-muted-foreground">
          {t`Sending needs ses:SendEmail. Reading this page needs ses:GetAccount, ses:ListEmailIdentities and ses:ListSuppressedDestinations; verifying a sender needs ses:CreateEmailIdentity; leaving the sandbox needs ses:PutAccountDetails; and asking for a bigger quota needs support:CreateCase on a Business plan or higher.`}
        </p>
      </div>
    )
  }

  const used = account.max_24_hour_send
    ? Math.round((account.sent_last_24_hours / account.max_24_hour_send) * 100)
    : 0

  return (
    <div className="flex flex-col gap-8">
      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`What Amazon allows`}</h2>

        <div className="grid gap-3 sm:grid-cols-3">
          <div className="rounded-xl border border-border px-4 py-3">
            <p className="text-lg font-semibold">
              {account.max_24_hour_send < 0
                ? t`no limit`
                : account.max_24_hour_send.toLocaleString()}
            </p>
            <p className="text-xs text-muted-foreground">{t`messages a day`}</p>
          </div>
          <div className="rounded-xl border border-border px-4 py-3">
            <p className="text-lg font-semibold">
              {account.sent_last_24_hours.toLocaleString()}
              {account.max_24_hour_send > 0 && (
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                  {used}%
                </span>
              )}
            </p>
            <p className="text-xs text-muted-foreground">{t`sent in the last day`}</p>
          </div>
          <div className="rounded-xl border border-border px-4 py-3">
            <p className="text-lg font-semibold">{account.max_send_rate}</p>
            <p className="text-xs text-muted-foreground">{t`a second, at most`}</p>
          </div>
        </div>

        {!account.sending_enabled && (
          <p className="flex items-start gap-2 text-sm text-destructive">
            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
            {t`Sending is off on this account (${account.enforcement_status}). Nothing goes out until it is on again — Amazon switches it off itself when an account is in trouble.`}
          </p>
        )}

        <div>
          <Button
            variant={account.sending_enabled ? "outline" : "default"}
            onClick={() => {
              setBusy(true)
              void setSesSending(!account.sending_enabled, siteId)
                .then(() => {
                  toast.success(
                    account.sending_enabled
                      ? t`Sending stopped`
                      : t`Sending resumed`
                  )
                  load()
                })
                .catch((error) =>
                  toast.error(
                    error instanceof ApiError ? error.message : t`Could not do it`
                  )
                )
                .finally(() => setBusy(false))
            }}
            disabled={busy}
          >
            {account.sending_enabled ? t`Stop all sending` : t`Resume sending`}
          </Button>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <div>
          <h2 className="text-sm font-medium">{t`Sandbox`}</h2>
          <p className="text-sm text-muted-foreground">
            {account.production_access
              ? t`This account is out of the sandbox: it can write to anybody.`
              : t`A new SES account starts in the sandbox, where it may only write to addresses you have verified below. Asking to leave it is how you can write to your visitors.`}
          </p>
        </div>

        {account.production_access ? (
          <p className="flex items-center gap-2 text-sm">
            <CheckCircle2 className="size-4 text-emerald-600" />
            {t`Full access`}
          </p>
        ) : (
          <div className="flex max-w-xl flex-col gap-4 rounded-xl border border-border p-4">
            {account.review_status && (
              <p className="text-sm text-muted-foreground">
                {t`Last request: ${account.review_status}`}
              </p>
            )}

            <div className="flex flex-col gap-2">
              <Label htmlFor="ses-mail-type">{t`What the mail is`}</Label>
              <Select
                value={request.mail_type}
                onValueChange={(value) =>
                  setRequest({ ...request, mail_type: value ?? "TRANSACTIONAL" })
                }
              >
                <SelectTrigger id="ses-mail-type">
                  <SelectValue>
                    {(value: string) =>
                      value === "MARKETING"
                        ? t`Newsletters and campaigns`
                        : t`Notifications people asked for`
                    }
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="TRANSACTIONAL">
                    {t`Notifications people asked for`}
                  </SelectItem>
                  <SelectItem value="MARKETING">
                    {t`Newsletters and campaigns`}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="ses-website">{t`Website`}</Label>
              <Input
                id="ses-website"
                value={request.website_url}
                onChange={(event) =>
                  setRequest({ ...request, website_url: event.target.value })
                }
                placeholder="https://example.com"
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="ses-use-case">{t`Why you send`}</Label>
              <Textarea
                id="ses-use-case"
                rows={4}
                value={request.use_case_description}
                onChange={(event) =>
                  setRequest({
                    ...request,
                    use_case_description: event.target.value,
                  })
                }
                placeholder={t`Who receives it, how they came to be on the list, and how they stop it. A person at Amazon reads this.`}
              />
            </div>

            <div>
              <Button
                onClick={() => void ask()}
                disabled={
                  busy ||
                  request.use_case_description.trim().length < 30 ||
                  !request.website_url.startsWith("http")
                }
              >
                {busy ? <Loader2 className="animate-spin" /> : null}
                {t`Ask to leave the sandbox`}
              </Button>
            </div>
          </div>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <div>
          <h2 className="text-sm font-medium">{t`Who you may send as`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`SES refuses to send from an address or domain it has not verified. Add an address and it gets a link; add a domain and it gets records to publish, which is what lets every address on it send.`}
          </p>
        </div>

        <div className="flex flex-wrap gap-2">
          <Input
            value={newIdentity}
            onChange={(event) => setNewIdentity(event.target.value)}
            placeholder={t`someone@example.com or example.com`}
            className="max-w-sm"
          />
          <Button
            variant="outline"
            onClick={() => void add()}
            disabled={busy || newIdentity.trim().length < 3}
          >
            <Plus /> {t`Verify`}
          </Button>
        </div>

        {identities.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`Nothing verified yet`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {identities.map((identity) => (
              <div key={identity.name} className="flex flex-col gap-2 px-4 py-3">
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">
                      {identity.name}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {identity.verified ? t`Verified` : t`Waiting`}
                      {identity.dkim_status ? ` · DKIM ${identity.dkim_status}` : ""}
                      {identity.mail_from_domain
                        ? ` · ${t`bounces to ${identity.mail_from_domain}`}`
                        : ""}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Remove`}
                    disabled={busy}
                    onClick={() =>
                      void deleteSesIdentity(identity.name, siteId)
                        .then(load)
                        .catch(() => toast.error(t`Could not remove it`))
                    }
                  >
                    <Trash2 />
                  </Button>
                </div>

                {identity.kind === "DOMAIN" && (
                  <DomainRecords
                    identity={identity}
                    siteId={siteId}
                    onChanged={load}
                  />
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="flex flex-col gap-3">
        <div>
          <h2 className="text-sm font-medium">{t`Addresses Amazon refuses`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`A message that bounced hard, or that somebody reported as spam. Amazon keeps this list itself and honours it whatever this panel does — a bounce costs money and complaints close accounts.`}
          </p>
        </div>

        {suppressed.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`None`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {suppressed.map((entry) => (
              <div
                key={entry.address}
                className="flex items-center gap-3 px-4 py-2.5"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm">{entry.address}</p>
                  <p className="text-xs text-muted-foreground">
                    {entry.reason === "COMPLAINT"
                      ? t`Reported it as spam`
                      : t`Did not arrive`}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() =>
                    void unsuppressSesAddress(entry.address, siteId)
                      .then(load)
                      .catch(() => toast.error(t`Could not unblock it`))
                  }
                >
                  {t`Unblock`}
                </Button>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}

/**
 * Everything a domain has to have in DNS, in one place, ready to paste.
 *
 * The whole point of this panel is that somebody sets up a sending domain
 * without reading Amazon's documentation: three CNAMEs so SES can sign, an MX
 * and an SPF line so bounces come back to a subdomain the site owns, and
 * DMARC, which Gmail and Yahoo have required of bulk senders since 2024 and
 * which nothing in AWS tells you is missing.
 */
function DomainRecords({
  identity,
  siteId,
  onChanged,
}: {
  identity: SesIdentity
  siteId?: string
  onChanged: () => void
}) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)
  const [bounce, setBounce] = React.useState(
    identity.mail_from_domain || `mail.${identity.name}`
  )

  const copy = (value: string, key: string) => {
    void navigator.clipboard.writeText(value).then(
      () => {
        setCopied(key)
        setTimeout(() => setCopied(null), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  // One block for a registrar that takes a paste, rows for one that does not.
  const asZoneFile = identity.records
    .map((r) => `${r.host}\t${r.kind}\t${r.value}`)
    .join("\n")

  const label = (purpose: string) =>
    ({
      dkim: t`Lets Amazon sign as you`,
      mail_from: t`Sends bounces back to you`,
      dmarc: t`What Gmail and Yahoo ask for`,
    })[purpose] ?? purpose

  const standing = (status: string) =>
    ({
      verified: t`published`,
      waiting: t`waiting for DNS`,
      failed: t`Amazon could not find it`,
      unchecked: t`Amazon does not check this one`,
    })[status] ?? status

  return (
    <div className="flex flex-col gap-3 rounded-lg bg-muted/40 p-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {t`Publish these at whoever holds the domain. Amazon finds them by itself, usually within the hour.`}
        </p>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => copy(asZoneFile, "all")}
        >
          {copied === "all" ? <Check /> : <Copy />}
          {t`Copy all`}
        </Button>
      </div>

      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <tbody>
            {identity.records.map((record) => {
              const line = `${record.host}\t${record.kind}\t${record.value}`
              return (
                <tr key={record.host + record.kind} className="align-top">
                  <td className="py-1 pr-3 font-mono whitespace-nowrap">
                    {record.kind}
                  </td>
                  <td className="py-1 pr-3 font-mono break-all">
                    {record.host}
                  </td>
                  <td className="py-1 pr-3 font-mono break-all">
                    {record.value}
                  </td>
                  <td className="py-1 pr-2 whitespace-nowrap text-muted-foreground">
                    {label(record.purpose)} · {standing(record.status)}
                  </td>
                  <td className="py-1">
                    <button
                      type="button"
                      aria-label={t`Copy`}
                      onClick={() => copy(line, line)}
                      className="text-muted-foreground hover:text-foreground"
                    >
                      {copied === line ? (
                        <Check className="size-3" />
                      ) : (
                        <Copy className="size-3" />
                      )}
                    </button>
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>

      {!identity.mail_from_domain && (
        <div className="flex flex-wrap items-end gap-2 border-t border-border pt-3">
          <div className="flex flex-col gap-1">
            <Label htmlFor={`mailfrom-${identity.name}`} className="text-xs">
              {t`Subdomain for bounces`}
            </Label>
            <Input
              id={`mailfrom-${identity.name}`}
              value={bounce}
              onChange={(event) => setBounce(event.target.value)}
              className="h-8 max-w-xs text-xs"
            />
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || !bounce.endsWith(identity.name)}
            onClick={() => {
              setBusy(true)
              void setSesMailFrom(identity.name, bounce, siteId)
                .then(onChanged)
                .catch((error) =>
                  toast.error(
                    error instanceof ApiError ? error.message : t`Could not set it`
                  )
                )
                .finally(() => setBusy(false))
            }}
          >
            {t`Set it up`}
          </Button>
          <p className="w-full text-xs text-muted-foreground">
            {t`Optional, and worth it: without one, bounces go to amazonses.com and a receiving server learns less about who really sent the message. Two more records appear above once it is set.`}
          </p>
        </div>
      )}
    </div>
  )
}
