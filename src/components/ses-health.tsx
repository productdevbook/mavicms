import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { AlertTriangle, CheckCircle2, Inbox, Send } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getSesHealth,
  getSesRequests,
  requestQuotaIncrease,
  type SesHealth,
  type SesRequest,
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
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
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"
import { Label } from "@/components/ui/label"
import {
  Progress,
  ProgressLabel,
  ProgressValue,
} from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import { Textarea } from "@/components/ui/textarea"
import { SesAnalyticsPanel } from "@/components/ses-analytics"

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

  const bar =
    standing === "danger"
      ? "[&_[data-slot=progress-indicator]]:bg-destructive"
      : standing === "watch"
        ? "[&_[data-slot=progress-indicator]]:bg-amber-500"
        : "[&_[data-slot=progress-indicator]]:bg-emerald-600"

  // Scaled to the pause line, because that is the number that matters:
  // halfway along it means halfway to being switched off.
  return (
    <div className="rounded-xl border px-4 py-3">
      <Progress value={Math.min(100, (rate / pauseAt) * 100)} className={bar}>
        <ProgressLabel>{label}</ProgressLabel>
        <ProgressValue className={cn("text-lg font-semibold", tone)}>
          {() => `${rate.toFixed(2)}%`}
        </ProgressValue>
      </Progress>
      <p className="mt-2 text-xs text-muted-foreground">
        {t`${count} of them · Amazon reviews at ${reviewAt}%, pauses at ${pauseAt}%`}
      </p>
    </div>
  )
}

/** Two weeks of sending, with what went wrong stacked on top of it. */
function Fortnight({ days }: { days: SesHealth["days"] }) {
  const { t } = useLingui()
  const most = Math.max(...days.map((day) => day.delivery_attempts), 1)

  return (
    <div className="flex flex-col gap-1">
      <div className="flex h-16 items-end gap-0.5">
        {days.map((day) => {
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
                style={{ height: `${(day.delivery_attempts / most) * 56}px` }}
              />
            </div>
          )
        })}
      </div>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>{days[0]?.day}</span>
        <span>{t`grey: sent · red: did not arrive`}</span>
        <span>{days[days.length - 1]?.day}</span>
      </div>
    </div>
  )
}

export function SesHealthPanel({ siteId }: { siteId?: string }) {
  const { t } = useLingui()

  const [health, setHealth] = React.useState<SesHealth | null>(null)
  const [requests, setRequests] = React.useState<SesRequest[] | null>(null)
  const [problem, setProblem] = React.useState<string | null>(null)
  const [requestProblem, setRequestProblem] = React.useState<string | null>(
    null
  )
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
        <Spinner className="size-6 text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <SesAnalyticsPanel siteId={siteId} />

      <Card>
        <CardHeader>
          <CardTitle>{t`How the sending is going`}</CardTitle>
          <CardDescription>
            {t`The last two weeks, which is as far back as Amazon keeps it. These two numbers are what an account gets suspended over.`}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {problem || !health ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyTitle>{problem ?? t`Could not ask Amazon`}</EmptyTitle>
              </EmptyHeader>
            </Empty>
          ) : health.delivery_attempts === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyTitle>{t`Nothing sent in the last two weeks`}</EmptyTitle>
              </EmptyHeader>
            </Empty>
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
                <Alert variant="destructive">
                  <AlertTriangle />
                  <AlertTitle>{t`Clean the list before sending again`}</AlertTitle>
                  <AlertDescription>
                    {t`Take off the addresses that bounced, and stop writing to anybody who has not opened anything in a year.`}
                  </AlertDescription>
                </Alert>
              )}

              {health.days.length > 1 && <Fortnight days={health.days} />}
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t`Asking for a bigger quota`}</CardTitle>
          <CardDescription>
            {t`Leaving the sandbox is the first increase. This is the one after it — Amazon treats it as a support request, and this opens one in the same words its console would.`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex max-w-xl flex-col gap-4">
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
                  setAsk({
                    ...ask,
                    use_case_description: event.target.value,
                  })
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
                {busy ? <Spinner /> : <Send />}
                {t`Ask for it`}
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t`What you have asked for`}</CardTitle>
        </CardHeader>
        <CardContent>
          {requestProblem ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyTitle>{requestProblem}</EmptyTitle>
              </EmptyHeader>
            </Empty>
          ) : !requests || requests.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Inbox />
                </EmptyMedia>
                <EmptyTitle>{t`Nothing asked for yet`}</EmptyTitle>
                <EmptyDescription>
                  {t`Requests opened from this page appear here with Amazon's answer.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <ItemGroup className="rounded-xl border">
              {requests.map((entry) => (
                <Item key={entry.id} size="sm">
                  <ItemMedia>
                    {entry.status === "resolved" ? (
                      <CheckCircle2 className="size-4 text-emerald-600" />
                    ) : (
                      <Spinner className="size-4 text-muted-foreground" />
                    )}
                  </ItemMedia>
                  <ItemContent>
                    <ItemTitle>{entry.subject}</ItemTitle>
                    <ItemDescription>
                      {entry.status} · {entry.created_at}
                    </ItemDescription>
                    {entry.latest && (
                      <ItemDescription className="line-clamp-3">
                        {entry.latest}
                      </ItemDescription>
                    )}
                  </ItemContent>
                </Item>
              ))}
            </ItemGroup>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
