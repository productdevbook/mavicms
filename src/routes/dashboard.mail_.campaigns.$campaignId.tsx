/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2, Pause, Send, Square } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  cancelCampaign,
  getCampaign,
  getMailLists,
  getMailTemplates,
  pauseCampaign,
  saveCampaign,
  startCampaign,
  testCampaign,
  type Campaign,
  type MailList,
  type MailTemplate,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"

export const Route = createFileRoute("/dashboard/mail_/campaigns/$campaignId")({
  component: CampaignRoute,
})

/** How often to look again while one is going out. */
const WATCH = 4000

function CampaignRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { campaignId } = Route.useParams()

  const [campaign, setCampaign] = React.useState<Campaign | null>(null)
  const [lists, setLists] = React.useState<MailList[]>([])
  const [templates, setTemplates] = React.useState<MailTemplate[]>([])
  const [busy, setBusy] = React.useState(false)
  const [testTo, setTestTo] = React.useState("")

  const load = React.useCallback(() => {
    getCampaign(campaignId)
      .then(setCampaign)
      .catch(() => toast.error(t`Could not load the campaign`))
  }, [campaignId, t])

  React.useEffect(load, [load])
  React.useEffect(() => {
    getMailLists().then(setLists).catch(() => setLists([]))
    getMailTemplates().then(setTemplates).catch(() => setTemplates([]))
  }, [])

  // Only while something is happening; a finished campaign is not polled.
  const running = campaign?.status === "running"
  React.useEffect(() => {
    if (!running) return
    const timer = setInterval(load, WATCH)
    return () => clearInterval(timer)
  }, [running, load])

  const save = async () => {
    if (!campaign) return
    setBusy(true)
    try {
      const saved = await saveCampaign(
        {
          name: campaign.name,
          subject: campaign.subject,
          body: campaign.body,
          template_id: campaign.template_id,
          lists: campaign.lists,
          send_at: campaign.send_at,
        },
        campaign.id
      )
      setCampaign(saved)
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    } finally {
      setBusy(false)
    }
  }

  const act = async (what: "send" | "pause" | "cancel") => {
    if (!campaign) return
    setBusy(true)
    try {
      const call =
        what === "send"
          ? startCampaign
          : what === "pause"
            ? pauseCampaign
            : cancelCampaign
      setCampaign(await call(campaign.id))
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not do it`
      )
    } finally {
      setBusy(false)
    }
  }

  const sendTest = async () => {
    if (!campaign) return
    setBusy(true)
    try {
      await testCampaign(campaign.id, testTo.trim())
      toast.success(t`Sent to ${testTo.trim()}`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not send it`
      )
    } finally {
      setBusy(false)
    }
  }

  if (!campaign) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const editable =
    campaign.status !== "running" && campaign.status !== "finished"

  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="mb-4 -ml-2"
        onClick={() => void navigate({ to: "/dashboard/mail" })}
      >
        <ArrowLeft /> {t`Mail`}
      </Button>

      <div className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{campaign.name}</h1>
          <p className="text-sm text-muted-foreground">
            {campaign.to_send > 0
              ? t`${campaign.sent} of ${campaign.to_send} sent, ${campaign.failed} refused, ${campaign.opened} opened, ${campaign.clicked} clicked`
              : t`Not sent yet`}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {campaign.status === "running" ? (
            <>
              <Button
                variant="outline"
                onClick={() => void act("pause")}
                disabled={busy}
              >
                <Pause /> {t`Pause`}
              </Button>
              <Button
                variant="outline"
                onClick={() => void act("cancel")}
                disabled={busy}
              >
                <Square /> {t`Stop`}
              </Button>
            </>
          ) : (
            <Button
              onClick={() => void act("send")}
              disabled={busy || campaign.status === "finished"}
            >
              {busy ? <Loader2 className="animate-spin" /> : <Send />}
              {campaign.status === "paused" ? t`Carry on` : t`Send it`}
            </Button>
          )}
        </div>
      </div>

      <div className="flex max-w-3xl flex-col gap-6">
        <div className="flex flex-col gap-2">
          <Label htmlFor="c-name">{t`Name`}</Label>
          <Input
            id="c-name"
            value={campaign.name}
            disabled={!editable}
            onChange={(event) =>
              setCampaign({ ...campaign, name: event.target.value })
            }
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="c-subject">{t`Subject`}</Label>
          <Input
            id="c-subject"
            value={campaign.subject}
            disabled={!editable}
            onChange={(event) =>
              setCampaign({ ...campaign, subject: event.target.value })
            }
          />
        </div>

        <div className="flex flex-col gap-2">
          <Label>{t`Who gets it`}</Label>
          {lists.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {t`No lists yet — make one first.`}
            </p>
          ) : (
            lists.map((list) => (
              <Label key={list.id} className="flex items-center gap-2 font-normal">
                <input
                  type="checkbox"
                  disabled={!editable}
                  checked={campaign.lists.includes(list.id)}
                  onChange={(event) =>
                    setCampaign({
                      ...campaign,
                      lists: event.target.checked
                        ? [...campaign.lists, list.id]
                        : campaign.lists.filter((id) => id !== list.id),
                    })
                  }
                />
                {list.name}
                <span className="text-xs text-muted-foreground">
                  {t`${list.confirmed} confirmed`}
                </span>
              </Label>
            ))
          )}
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="c-template">{t`Letterhead`}</Label>
          <select
            id="c-template"
            disabled={!editable}
            value={campaign.template_id ?? ""}
            onChange={(event) =>
              setCampaign({
                ...campaign,
                template_id: event.target.value || null,
              })
            }
            className="h-9 rounded-md border border-border bg-transparent px-3 text-sm"
          >
            <option value="">{t`None — send the body as it is`}</option>
            {templates.map((template) => (
              <option key={template.id} value={template.id}>
                {template.name}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="c-body">{t`The letter`}</Label>
          <Textarea
            id="c-body"
            rows={16}
            className="font-mono text-xs"
            value={campaign.body}
            disabled={!editable}
            onChange={(event) =>
              setCampaign({ ...campaign, body: event.target.value })
            }
          />
          <p className="text-sm text-muted-foreground">
            {t`HTML. {{ name }}, {{ email }} and {{ unsubscribe_url }} are filled in for each person, along with any field you keep about them. A plain-text part is made from this automatically.`}
          </p>
        </div>

        {editable && (
          <div>
            <Button onClick={() => void save()} disabled={busy}>
              {busy ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
          </div>
        )}

        <div className="flex flex-col gap-3 border-t border-border pt-6">
          <div>
            <h2 className="text-sm font-medium">{t`Send yourself one`}</h2>
            <p className="text-sm text-muted-foreground">
              {t`Rendered exactly as a subscriber would get it, unsubscribe link and all.`}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Input
              type="email"
              value={testTo}
              onChange={(event) => setTestTo(event.target.value)}
              placeholder={t`Where to send it`}
              className="max-w-xs"
            />
            <Button
              variant="outline"
              onClick={() => void sendTest()}
              disabled={busy || !testTo.includes("@")}
            >
              <Send /> {t`Send`}
            </Button>
          </div>
        </div>
      </div>
    </>
  )
}
