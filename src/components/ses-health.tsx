import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { AlertTriangle, CheckCircle2, Loader2, Send } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getSesHealth,
  getSesRequests,
  requestQuotaIncrease,
  setSesSending,
  type SesHealth,
  type SesRequest,
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"

/**
 * How the account has been behaving, and what to do when it needs to send
 * more.
 *
 * The two numbers on this page are the ones Amazon suspends accounts over.
 * They are not shown anywhere else somebody running a website would look, and
 * by the time a suspension notice arrives it is too late to act on them — so
 * they are here, with Amazon's own thresholds beside them rather than a green
 * tick that means nothing.
 */

/** A rate against the two lines Amazon draws through it. */
function Rate({
  label,
  rate,
  standing,
  reviewAt,
  pauseAt,
  count,
}: {
  label: string
  rate: number
  standing: string
  reviewAt: number
  pauseAt: number
  count: number
}) {
  const { t } = useLingui()

  const tone =
    standing === "danger"
      ? "text-destructive"
      : standing === "watch"
        ? "text-amber-600"
        : "text-emerald-600"

  // The bar is scaled to the pause line, because that is the number that
  // matters: halfway along it means halfway to being switched off.
  const width = Math.min(100, (rate / pauseAt) * 100)

  return (
    <div className="rounded-xl border border-border px-4 py-3">
      <div className="flex items-baseline justify-between gap-2">
        <p className="text-sm font-medium">{label}</p>
        <p className={cn("text-lg font-semibold", tone)}>{rate.toFixed(2)}%</p>
      </div>

      <div className="my-2 h-1.5 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full",
            standing === "danger"
              ? "bg-destructive"
              : standing === "watch"
                ? "bg-amber-500"
                : "bg-emerald-600"
          )}
          style={{ width: `${width}%` }}
        />
      </div>

      <p className="text-xs text-muted-foreground">
        {t`${count} of them · Amazon reviews at ${reviewAt}%, pauses at ${pauseAt}%`}
      </p>
    </div>
  )
}

export function SesHealthPanel({
  siteId,
  sendingEnabled,
  onSendingChanged,
}: {
  siteId?: string
  sendingEnabled: boolean
  onSendingChanged: () => void
}) {
  const { t } = useLingui()

  const [health, setHealth] = React.useState<SesHealth | null>(null)
  const [requests, setRequests] = React.useState<SesRequest[] | null>(null)
  const [problem, setProblem] = React.useState<string | null>(null)
  const [requestProblem, setRequestProblem] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState(false)

  const [ask, setAsk] = React.useState({
    daily_limit: 50000,
    send_rate: 14,
    website_url: "",
    use_case_description: "",
  })

  const load = React.useCallback(() => {
    getSesHealth(siteId)
      .then((found) => {
        setHealth(found)
        setProblem(null)
      })
      .catch((error) => {
        setHealth(null)
        setProblem(
          error instanceof ApiError ? error.message : t`Could not ask Amazon`
        )
      })
      .finally(() => setLoading(false))

    // Opening a support case needs both a support plan and permission, and
    // most keys have neither. The reason is shown once, here, rather than as
    // a failure the first time somebody tries to ask for anything.
    getSesRequests(siteId)
      .then((found) => {
        setRequests(found)
        setRequestProblem(null)
      })
      .catch((error) => {
        setRequests(null)
        setRequestProblem(
          error instanceof ApiError ? error.message : t`Could not ask Amazon`
        )
      })
  }, [siteId, t])

  React.useEffect(load, [load])

  const toggleSending = async () => {
    setBusy(true)
    try {
      await setSesSending(!sendingEnabled, siteId)
      onSendingChanged()
      toast.success(sendingEnabled ? t`Sending stopped` : t`Sending resumed`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not do it`
      )
    } finally {
      setBusy(false)
    }
  }

  const askForMore = async () => {
    setBusy(true)
    try {
      await requestQuotaIncrease({ ...ask, language: "en" }, siteId)
      toast.success(t`Asked. Amazon usually answers within a day or two.`)
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
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-8">
      <section className="flex flex-col gap-3">
        <div>
          <h2 className="text-sm font-medium">{t`How the sending is going`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`The last two weeks, which is as far back as Amazon keeps it. These two numbers are what an account gets suspended over.`}
          </p>
        </div>

        {problem || !health ? (
          <p className="rounded-xl border border-dashed border-border px-4 py-6 text-sm text-muted-foreground">
            {problem ?? t`Could not ask Amazon`}
          </p>
        ) : health.delivery_attempts === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`Nothing sent in the last two weeks`}
          </p>
        ) : (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              <Rate
                label={t`Did not arrive`}
                rate={health.bounce_rate}
                standing={health.bounce_standing}
                reviewAt={health.bounce_review_at}
                pauseAt={health.bounce_pause_at}
                count={health.bounces}
              />
              <Rate
                label={t`Reported as spam`}
                rate={health.complaint_rate}
                standing={health.complaint_standing}
                reviewAt={health.complaint_review_at}
                pauseAt={health.complaint_pause_at}
                count={health.complaints}
              />
            </div>

            <p className="text-sm text-muted-foreground">
              {t`${health.delivery_attempts} attempted, ${health.rejects} refused before sending.`}
            </p>

            {(health.bounce_standing !== "healthy" ||
              health.complaint_standing !== "healthy") && (
              <p className="flex items-start gap-2 text-sm text-amber-700 dark:text-amber-500">
                <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                {t`Clean the list before sending again: take off the addresses that bounced, and stop writing to anybody who has not opened anything in a year.`}
              </p>
            )}

            {health.days.length > 1 && (
              <div className="flex h-16 items-end gap-0.5">
                {health.days.map((day) => {
                  const most = Math.max(
                    ...health.days.map((d) => d.delivery_attempts),
                    1
                  )
                  const bad = day.bounces + day.complaints
                  return (
                    <div
                      key={day.day}
                      title={t`${day.day}: ${day.delivery_attempts} sent, ${day.bounces} did not arrive`}
                      className="flex flex-1 flex-col justify-end"
                    >
                      {bad > 0 && (
                        <div
                          className="w-full rounded-t-sm bg-destructive"
                          style={{ height: `${(bad / most) * 56}px` }}
                        />
                      )}
                      <div
                        className="w-full rounded-b-sm bg-muted-foreground/30"
                        style={{
                          height: `${(day.delivery_attempts / most) * 56}px`,
                        }}
                      />
                    </div>
                  )
                })}
              </div>
            )}
          </>
        )}

        <div>
          <Button
            variant={sendingEnabled ? "outline" : "default"}
            onClick={() => void toggleSending()}
            disabled={busy}
          >
            {sendingEnabled ? t`Stop all sending` : t`Resume sending`}
          </Button>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <div>
          <h2 className="text-sm font-medium">{t`Asking for a bigger quota`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`Leaving the sandbox is the first increase. This is the one after it — Amazon treats it as a support request, and this opens one in the same words its console would.`}
          </p>
        </div>

        <div className="flex max-w-xl flex-col gap-4 rounded-xl border border-border p-4">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="quota-daily">{t`Messages a day`}</Label>
              <Input
                id="quota-daily"
                type="number"
                min={1}
                value={ask.daily_limit}
                onChange={(event) =>
                  setAsk({ ...ask, daily_limit: Number(event.target.value) })
                }
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="quota-rate">{t`Messages a second`}</Label>
              <Input
                id="quota-rate"
                type="number"
                min={0}
                value={ask.send_rate}
                onChange={(event) =>
                  setAsk({ ...ask, send_rate: Number(event.target.value) })
                }
              />
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="quota-site">{t`Website`}</Label>
            <Input
              id="quota-site"
              value={ask.website_url}
              onChange={(event) =>
                setAsk({ ...ask, website_url: event.target.value })
              }
              placeholder="https://example.com"
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="quota-why">{t`Why you need it`}</Label>
            <Textarea
              id="quota-why"
              rows={5}
              value={ask.use_case_description}
              onChange={(event) =>
                setAsk({ ...ask, use_case_description: event.target.value })
              }
              placeholder={t`Who receives the mail, how they came to be on the list, how they stop it, and what you do about bounces. A person at Amazon reads this and decides on it.`}
            />
          </div>

          <div>
            <Button
              onClick={() => void askForMore()}
              disabled={
                busy ||
                ask.daily_limit < 1 ||
                ask.use_case_description.trim().length < 50 ||
                !ask.website_url.startsWith("http")
              }
            >
              {busy ? <Loader2 className="animate-spin" /> : <Send />}
              {t`Ask for it`}
            </Button>
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`What you have asked for`}</h2>

        {requestProblem ? (
          <p className="rounded-xl border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
            {requestProblem}
          </p>
        ) : !requests || requests.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`Nothing asked for yet`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {requests.map((entry) => (
              <div key={entry.id} className="flex items-start gap-3 px-4 py-3">
                {entry.status === "resolved" ? (
                  <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600" />
                ) : (
                  <Loader2 className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{entry.subject}</p>
                  <p className="text-xs text-muted-foreground">
                    {entry.status} · {entry.created_at}
                  </p>
                  {entry.latest && (
                    <p className="mt-1 line-clamp-3 text-xs text-muted-foreground">
                      {entry.latest}
                    </p>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
