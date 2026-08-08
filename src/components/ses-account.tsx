import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronDown,
  Copy,
  Globe,
  Mail,
  Plus,
  ShieldOff,
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
  type SesDnsRecord,
  type SesIdentity,
  type SesSuppressed,
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item"
import { Label } from "@/components/ui/label"
import { Spinner } from "@/components/ui/spinner"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
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
  const [suppressed, setSuppressed] = React.useState<SesSuppressed[]>([])
  const [problem, setProblem] = React.useState<string | null>(null)
  // Only loading when there is something to load; with no settings the
  // panel has nothing to ask and nothing to wait for.
  const [loading, setLoading] = React.useState(ready)
  const [busy, setBusy] = React.useState(false)

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

    getSesSuppressed(siteId)
      .then(setSuppressed)
      .catch(() => setSuppressed([]))
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
      toast.error(error instanceof ApiError ? error.message : t`Could not ask`)
    } finally {
      setBusy(false)
    }
  }

  if (loading) {
    return (
      <div className="flex justify-center py-12">
        <Spinner className="size-6 text-muted-foreground" />
      </div>
    )
  }

  if (!ready) {
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Mail />
          </EmptyMedia>
          <EmptyTitle>{t`Not connected yet`}</EmptyTitle>
          <EmptyDescription>
            {t`Fill in the region and the keys above and save them. Everything Amazon knows about this account then appears here: how much sending is left today, whether it is still in the sandbox, which senders are verified, and how the last two weeks went.`}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  if (problem || !account) {
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <AlertTriangle />
          </EmptyMedia>
          <EmptyTitle>{problem ?? t`Could not ask Amazon`}</EmptyTitle>
          <EmptyDescription>
            {t`Sending needs ses:SendEmail. Reading this page needs ses:GetAccount, ses:ListEmailIdentities and ses:ListSuppressedDestinations; verifying a sender needs ses:CreateEmailIdentity; leaving the sandbox needs ses:PutAccountDetails; and asking for a bigger quota needs support:CreateCase on a Business plan or higher.`}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  }

  const used = account.max_24_hour_send
    ? Math.round((account.sent_last_24_hours / account.max_24_hour_send) * 100)
    : 0

  return (
    <div className="flex flex-col gap-6">
      <Card>
        <CardHeader>
          <CardTitle>{t`What Amazon allows`}</CardTitle>
          <CardDescription>
            {t`The ceiling on this account today. Sending stops at it rather than queueing.`}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="grid gap-3 sm:grid-cols-3">
            <Figure
              value={
                account.max_24_hour_send < 0
                  ? t`no limit`
                  : account.max_24_hour_send.toLocaleString()
              }
              label={t`messages a day`}
            />
            <Figure
              value={account.sent_last_24_hours.toLocaleString()}
              aside={account.max_24_hour_send > 0 ? `${used}%` : undefined}
              label={t`sent in the last day`}
            />
            <Figure
              value={String(account.max_send_rate)}
              label={t`a second, at most`}
            />
          </div>

          {!account.sending_enabled && (
            <Alert variant="destructive">
              <ShieldOff />
              <AlertTitle>{t`Sending is off on this account`}</AlertTitle>
              <AlertDescription>
                {t`Nothing goes out until it is on again. Amazon switches it off itself when an account is in trouble (${account.enforcement_status}).`}
              </AlertDescription>
            </Alert>
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
                      error instanceof ApiError
                        ? error.message
                        : t`Could not do it`
                    )
                  )
                  .finally(() => setBusy(false))
              }}
              disabled={busy}
            >
              {account.sending_enabled
                ? t`Stop all sending`
                : t`Resume sending`}
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t`Sandbox`}</CardTitle>
          <CardDescription>
            {account.production_access
              ? t`This account is out of the sandbox: it can write to anybody.`
              : t`A new SES account starts in the sandbox, where it may only write to addresses you have verified below. Asking to leave it is how you can write to your visitors.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {account.production_access ? (
            <Alert>
              <CheckCircle2 className="text-emerald-600" />
              <AlertTitle>{t`Full access`}</AlertTitle>
              <AlertDescription>
                {t`Anybody can be written to, up to the limits above.`}
              </AlertDescription>
            </Alert>
          ) : (
            <div className="flex max-w-xl flex-col gap-4">
              {account.review_status && (
                <Alert>
                  <AlertTitle>{t`Last request: ${account.review_status}`}</AlertTitle>
                </Alert>
              )}

              <div className="flex flex-col gap-2">
                <Label htmlFor="ses-mail-type">{t`What the mail is`}</Label>
                <Select
                  value={request.mail_type}
                  onValueChange={(value) =>
                    setRequest({
                      ...request,
                      mail_type: value ?? "TRANSACTIONAL",
                    })
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
                  {busy ? <Spinner /> : null}
                  {t`Ask to leave the sandbox`}
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <SesSendersPanel siteId={siteId} />

      <Card>
        <CardHeader>
          <CardTitle>{t`Addresses Amazon refuses`}</CardTitle>
          <CardDescription>
            {t`A message that bounced hard, or that somebody reported as spam. Amazon keeps this list itself and honours it whatever this panel does — a bounce costs money and complaints close accounts.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {suppressed.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <CheckCircle2 />
                </EmptyMedia>
                <EmptyTitle>{t`None`}</EmptyTitle>
                <EmptyDescription>
                  {t`Amazon is refusing nobody on this account.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <ItemGroup className="rounded-xl border">
              {suppressed.map((entry) => (
                <Item key={entry.address} size="sm">
                  <ItemContent>
                    <ItemTitle className="font-mono">{entry.address}</ItemTitle>
                    <ItemDescription>
                      {entry.reason === "COMPLAINT"
                        ? t`Reported it as spam`
                        : t`Did not arrive`}{" "}
                      · {entry.since}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
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
                  </ItemActions>
                </Item>
              ))}
            </ItemGroup>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

/** One number with what it counts underneath. */
function Figure({
  value,
  label,
  aside,
}: {
  value: string
  label: string
  aside?: string
}) {
  return (
    <div className="rounded-xl border px-4 py-3">
      <p className="text-lg font-semibold">
        {value}
        {aside && (
          <span className="ml-2 text-xs font-normal text-muted-foreground">
            {aside}
          </span>
        )}
      </p>
      <p className="text-xs text-muted-foreground">{label}</p>
    </div>
  )
}

/** Where a record or an identity stands, in one word and one colour. */
function Standing({ status }: { status: string }) {
  const { t } = useLingui()

  const shown: Record<string, { label: string; className: string }> = {
    verified: {
      label: t`published`,
      className: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
    },
    waiting: { label: t`waiting for DNS`, className: "" },
    failed: {
      label: t`Amazon could not find it`,
      className: "bg-destructive/10 text-destructive",
    },
    unchecked: {
      label: t`Amazon does not check this one`,
      className: "text-muted-foreground",
    },
  }
  const it = shown[status] ?? { label: status, className: "" }

  return (
    <Badge
      variant={it.className ? "outline" : "secondary"}
      className={cn("font-normal whitespace-nowrap", it.className)}
    >
      {it.label}
    </Badge>
  )
}

/** One verified sender, and — for a domain — everything it has to publish. */
function Identity({
  identity,
  siteId,
  busy,
  onChanged,
}: {
  identity: SesIdentity
  siteId?: string
  busy: boolean
  onChanged: () => void
}) {
  const { t } = useLingui()
  const isDomain = identity.kind === "DOMAIN"

  return (
    <div className="rounded-xl border">
      <div className="flex items-center gap-3 px-4 py-3">
        {isDomain ? (
          <Globe className="size-4 shrink-0 text-muted-foreground" />
        ) : (
          <Mail className="size-4 shrink-0 text-muted-foreground" />
        )}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium">{identity.name}</p>
          {identity.mail_from_domain && (
            <p className="truncate text-xs text-muted-foreground">
              {t`bounces to ${identity.mail_from_domain}`}
            </p>
          )}
        </div>
        <Badge
          variant="outline"
          className={cn(
            "font-normal whitespace-nowrap",
            identity.verified &&
              "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
          )}
        >
          {identity.verified ? t`can send` : t`waiting`}
        </Badge>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t`Remove`}
          disabled={busy}
          onClick={() =>
            void deleteSesIdentity(identity.name, siteId)
              .then(onChanged)
              .catch(() => toast.error(t`Could not remove it`))
          }
        >
          <Trash2 />
        </Button>
      </div>

      {/* Open while a domain still needs something published, which is when
          the records are the only thing on the page worth reading; closed once
          it can send, so five working domains are not five walls of DNS. */}
      {isDomain && (
        <Collapsible defaultOpen={!identity.verified}>
          <CollapsibleTrigger
            render={
              <button
                type="button"
                className="flex w-full items-center gap-2 border-t px-4 py-2 text-xs text-muted-foreground hover:text-foreground"
              />
            }
          >
            <ChevronDown className="size-3 transition-transform group-data-[panel-open]:rotate-180" />
            {t`DNS records`} ({identity.records.length})
          </CollapsibleTrigger>
          <CollapsibleContent>
            <DomainRecords
              identity={identity}
              siteId={siteId}
              onChanged={onChanged}
            />
          </CollapsibleContent>
        </Collapsible>
      )}
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
 *
 * Names and values never wrap. Breaking a DNS name across two lines is the one
 * thing this table must not do — it is read by eye and typed into somebody
 * else's control panel, and `k3n7._domainkey.` on one line reads as complete.
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

  const groups: { purpose: string; title: string; note: string }[] = [
    {
      purpose: "dkim",
      title: t`Ownership and signing`,
      note: t`Publishing these is what proves the domain is yours, and what lets Amazon sign as you. Nothing sends until they are up.`,
    },
    {
      purpose: "mail_from",
      title: t`Bounces`,
      note: t`Sends refusals back to a subdomain you own instead of to amazonses.com, which is what makes the visible sender and the envelope sender agree.`,
    },
    {
      purpose: "dmarc",
      title: t`DMARC`,
      note: t`What Gmail and Yahoo have asked of bulk senders since 2024. It starts at p=none: a policy that rejects before anybody has read a report is how a domain quietly stops delivering its own mail.`,
    },
  ]

  return (
    <div className="flex flex-col gap-4 border-t px-4 py-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground">
          {t`Publish these at whoever holds the domain. Amazon finds them by itself, usually within the hour.`}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => copy(asZoneFile, "all")}
        >
          {copied === "all" ? <Check /> : <Copy />}
          {t`Copy all`}
        </Button>
      </div>

      {groups.map((group) => {
        const rows = identity.records.filter((r) => r.purpose === group.purpose)

        // The offer to set bounces up stands where its records will appear, so
        // a domain half-way through setup reads in the same order as a
        // finished one.
        if (group.purpose === "mail_from" && rows.length === 0) {
          return (
            <div key={group.purpose} className="flex flex-col gap-1.5">
              <p className="text-xs font-medium">{group.title}</p>
              <p className="text-xs text-muted-foreground">
                {t`Optional, and worth it: without one, bounces go to amazonses.com and a receiving server learns less about who really sent the message. Two records appear here once it is set.`}
              </p>
              <MailFromSetup
                identity={identity}
                siteId={siteId}
                onChanged={onChanged}
              />
            </div>
          )
        }

        if (rows.length === 0) return null
        return (
          <div key={group.purpose} className="flex flex-col gap-1.5">
            <p className="text-xs font-medium">{group.title}</p>
            <p className="text-xs text-muted-foreground">{group.note}</p>
            <RecordTable rows={rows} copied={copied} onCopy={copy} />
          </div>
        )
      })}
    </div>
  )
}

function MailFromSetup({
  identity,
  siteId,
  onChanged,
}: {
  identity: SesIdentity
  siteId?: string
  onChanged: () => void
}) {
  const { t } = useLingui()
  const [busy, setBusy] = React.useState(false)
  const [bounce, setBounce] = React.useState(`mail.${identity.name}`)

  return (
    <div className="flex flex-col gap-2 rounded-lg border bg-muted/40 p-3">
      <div className="flex flex-wrap items-end gap-2">
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
                  error instanceof ApiError
                    ? error.message
                    : t`Could not set it`
                )
              )
              .finally(() => setBusy(false))
          }}
        >
          {t`Set it up`}
        </Button>
      </div>
    </div>
  )
}

function RecordTable({
  rows,
  copied,
  onCopy,
}: {
  rows: SesDnsRecord[]
  copied: string | null
  onCopy: (value: string, key: string) => void
}) {
  const { t } = useLingui()

  return (
    <div className="overflow-x-auto rounded-lg border">
      <Table className="text-xs">
        <TableHeader>
          <TableRow>
            <TableHead className="h-8">{t`Type`}</TableHead>
            <TableHead className="h-8">{t`Name`}</TableHead>
            <TableHead className="h-8">{t`Value`}</TableHead>
            <TableHead className="h-8">{t`Status`}</TableHead>
            <TableHead className="h-8 w-8" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((record) => {
            const line = `${record.host}\t${record.kind}\t${record.value}`
            return (
              <TableRow key={record.host + record.kind + record.value}>
                <TableCell className="py-1.5 font-mono whitespace-nowrap">
                  {record.kind}
                </TableCell>
                <TableCell className="py-1.5 font-mono whitespace-nowrap">
                  {record.host}
                </TableCell>
                <TableCell className="py-1.5 font-mono whitespace-nowrap">
                  {record.value}
                </TableCell>
                <TableCell className="py-1.5">
                  <Standing status={record.status} />
                </TableCell>
                <TableCell className="py-1.5">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Copy`}
                    onClick={() => onCopy(line, line)}
                  >
                    {copied === line ? <Check /> : <Copy />}
                  </Button>
                </TableCell>
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    </div>
  )
}

/**
 * The senders, on their own.
 *
 * Split out because a site borrowing the server's Amazon account gets exactly
 * this and nothing else: the account's quota, its reputation reports and its
 * suppression list belong to whoever runs the server, and that suppression
 * list is every other site's correspondents by name.
 */
export function SesSendersPanel({
  siteId,
  borrowed = false,
}: {
  siteId?: string
  /** Sending through the server's account rather than one of this site's. */
  borrowed?: boolean
}) {
  const { t } = useLingui()
  const [identities, setIdentities] = React.useState<SesIdentity[]>([])
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState(false)
  const [newIdentity, setNewIdentity] = React.useState("")

  const load = React.useCallback(() => {
    getSesIdentities(siteId)
      .then(setIdentities)
      .catch(() => setIdentities([]))
      .finally(() => setLoading(false))
  }, [siteId])

  React.useEffect(load, [load])

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
        <Spinner className="size-6 text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      {borrowed ? (
        <div className="rounded-xl border border-border bg-muted/40 p-4">
          <p className="text-sm font-medium">{t`Sending through the server's account`}</p>
          <p className="mt-1 text-sm text-muted-foreground">
            {t`You do not need an Amazon account. Add the domain you send from below, publish the records it gives you, and your mail goes out as your own domain — the server only lends the account behind it.`}
          </p>
        </div>
      ) : null}

      <Card>
    <CardHeader>
      <CardTitle>{t`Who you may send as`}</CardTitle>
      <CardDescription>
        {t`SES refuses to send from an address or domain it has not verified. Add an address and it gets a link; add a domain and it gets records to publish, which is what lets every address on it send.`}
      </CardDescription>
    </CardHeader>
    <CardContent className="flex flex-col gap-4">
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
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Globe />
            </EmptyMedia>
            <EmptyTitle>{t`Nothing verified yet`}</EmptyTitle>
            <EmptyDescription>
              {t`Add the domain you send from. A domain covers every address on it, so it is worth doing before adding addresses one at a time.`}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="flex flex-col gap-3">
          {identities.map((identity) => (
            <Identity
              key={identity.name}
              identity={identity}
              siteId={siteId}
              busy={busy}
              onChanged={load}
            />
          ))}
        </div>
      )}
    </CardContent>
  </Card>
    </div>
  )
}
