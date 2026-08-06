import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { CheckCircle2, Radio } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getSesDeliverability,
  setupSesEvents,
  type SesDeliverability,
} from "@/lib/api"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
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
import { Spinner } from "@/components/ui/spinner"

/** One number and what it means. */
function Figure({
  label,
  value,
  rate,
}: {
  label: string
  value: number
  rate?: number
}) {
  return (
    <div className="rounded-xl border px-4 py-3">
      <p className="text-lg font-semibold">
        {value.toLocaleString()}
        {rate !== undefined && value > 0 && (
          <span className="ml-2 text-xs font-normal text-muted-foreground">
            {rate.toFixed(1)}%
          </span>
        )}
      </p>
      <p className="text-xs text-muted-foreground">{label}</p>
    </div>
  )
}

/**
 * What actually happened to the mail, from Amazon rather than from guesswork.
 *
 * Two different things, side by side and on purpose. The open and click
 * counts a campaign shows come from a pixel and a redirect, which a reader
 * with images off never trips. These come from SES itself: it knows what was
 * delivered, what bounced, what somebody reported — and there is no way to
 * learn that from this side of the wire.
 *
 * None of it arrives until the event pipeline exists, so the button that
 * builds it is here rather than buried in settings: a page that shows zeroes
 * without saying why is worse than one that says what to press.
 */
export function SesAnalyticsPanel({ siteId }: { siteId?: string }) {
  const { t } = useLingui()

  const [figures, setFigures] = React.useState<SesDeliverability | null>(null)
  const [problem, setProblem] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(true)
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    getSesDeliverability(siteId)
      .then((found) => {
        setFigures(found)
        setProblem(null)
      })
      .catch((error) => {
        setFigures(null)
        setProblem(
          error instanceof ApiError ? error.message : t`Could not ask Amazon`
        )
      })
      .finally(() => setLoading(false))
  }, [siteId, t])

  React.useEffect(load, [load])

  const build = async () => {
    setBusy(true)
    try {
      const built = await setupSesEvents(siteId)
      toast.success(
        t`Wired up. Events go to ${built.configuration_set} and arrive here.`
      )
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not set it up`
      )
    } finally {
      setBusy(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t`What Amazon saw`}</CardTitle>
        <CardDescription>
          {t`The last thirty days, from SES itself. Delivered, bounced and reported are things only Amazon knows — a pixel in the letter cannot tell you any of them.`}
        </CardDescription>
        <CardAction>
          <Button
            variant="outline"
            onClick={() => void build()}
            disabled={busy}
          >
            {busy ? <Spinner /> : <Radio />}
            {t`Wire up the events`}
          </Button>
        </CardAction>
      </CardHeader>

      <CardContent>
        {loading ? (
          <div className="flex justify-center py-8">
            <Spinner className="size-5 text-muted-foreground" />
          </div>
        ) : problem || !figures ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <Radio />
              </EmptyMedia>
              <EmptyTitle>{problem ?? t`Could not ask Amazon`}</EmptyTitle>
              <EmptyDescription>
                {t`These figures come from Amazon's deliverability manager, which has to be switched on for the account. "Wire up the events" does that, along with everything else that has to exist before an event can reach this site.`}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : figures.sent === 0 ? (
          <Empty className="border">
            <EmptyHeader>
              <EmptyTitle>{t`Nothing sent in the last thirty days`}</EmptyTitle>
            </EmptyHeader>
          </Empty>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="grid gap-3 sm:grid-cols-4">
              <Figure label={t`sent`} value={figures.sent} />
              <Figure
                label={t`arrived`}
                value={figures.delivered}
                rate={figures.delivery_rate}
              />
              <Figure
                label={t`opened`}
                value={figures.opened}
                rate={figures.open_rate}
              />
              <Figure
                label={t`clicked`}
                value={figures.clicked}
                rate={figures.click_rate}
              />
            </div>

            <div className="grid gap-3 sm:grid-cols-3">
              <Figure
                label={t`gone for good`}
                value={figures.permanent_bounces}
              />
              <Figure
                label={t`failed for now`}
                value={figures.transient_bounces}
              />
              <Figure label={t`reported as spam`} value={figures.complaints} />
            </div>

            <Alert>
              <CheckCircle2 className="text-emerald-600" />
              <AlertTitle>{t`Bad addresses stop themselves`}</AlertTitle>
              <AlertDescription>
                {t`An address that goes for good, or reports the mail as spam, is blocked here the moment Amazon says so — without waiting for the next campaign to find out.`}
              </AlertDescription>
            </Alert>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
