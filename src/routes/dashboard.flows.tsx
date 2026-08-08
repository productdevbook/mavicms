/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  ArrowDown,
  ArrowUp,
  Copy,
  Loader2,
  Play,
  Plus,
  Trash2,
  Workflow,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createFlow,
  deleteFlow,
  getFlowRuns,
  getFlowVocabulary,
  getFlows,
  testFlow,
  updateFlow,
  type Flow,
  type FlowRun,
  type FlowVocabulary,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { FlowCanvas, type Piece } from "@/components/flows/flow-canvas"
import { StepSettings, TriggerSettings } from "@/components/flows/flow-settings"

export const Route = createFileRoute("/dashboard/flows")({
  component: FlowsRoute,
})

type Draft = {
  id: string | null
  name: string
  trigger_kind: string
  trigger_config: Record<string, unknown>
  enabled: boolean
  webhook_url: string | null
  steps: { action: string; config: Record<string, unknown>; on_error: string }[]
}

const BLANK: Draft = {
  id: null,
  name: "",
  trigger_kind: "form.submitted",
  trigger_config: {},
  enabled: true,
  webhook_url: null,
  steps: [{ action: "mail.send", config: {}, on_error: "stop" }],
}

function asDraft(flow: Flow): Draft {
  return {
    id: flow.id,
    name: flow.name,
    trigger_kind: flow.trigger_kind,
    trigger_config: (flow.trigger_config ?? {}) as Record<string, unknown>,
    enabled: flow.enabled,
    webhook_url: flow.webhook_url,
    steps: flow.steps.map((step) => ({
      action: step.action,
      config: (step.config ?? {}) as Record<string, unknown>,
      on_error: step.on_error ?? "stop",
    })),
  }
}

function FlowsRoute() {
  const { t } = useLingui()

  const [flows, setFlows] = React.useState<Flow[] | null>(null)
  const [vocabulary, setVocabulary] = React.useState<FlowVocabulary | null>(null)
  const [draft, setDraft] = React.useState<Draft | null>(null)
  const [chosen, setChosen] = React.useState<Piece | null>(null)
  const [runs, setRuns] = React.useState<FlowRun[]>([])
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    getFlows()
      .then(setFlows)
      .catch(() => setFlows([]))
    getFlowRuns()
      .then(setRuns)
      .catch(() => setRuns([]))
  }, [])

  React.useEffect(() => {
    load()
    getFlowVocabulary()
      .then(setVocabulary)
      .catch(() => setVocabulary(null))
  }, [load])

  const named: Record<string, string> = {
    "form.submitted": t`A form is filled in`,
    "post.published": t`Something is published`,
    schedule: t`Every so often`,
    webhook: t`An address is called`,
    "mail.send": t`Send an email`,
    "http.request": t`Call an address`,
    branch: t`Only carry on if`,
  }

  const save = async () => {
    if (!draft) return
    setBusy(true)
    try {
      const payload = {
        name: draft.name.trim(),
        trigger_kind: draft.trigger_kind,
        trigger_config: draft.trigger_config,
        enabled: draft.enabled,
        steps: draft.steps,
      }
      const saved = draft.id
        ? await updateFlow(draft.id, payload)
        : await createFlow(payload)
      setDraft(asDraft(saved))
      load()
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the flow`
      )
    } finally {
      setBusy(false)
    }
  }

  const tryIt = async () => {
    if (!draft?.id) {
      toast.error(t`Save it first`)
      return
    }
    setBusy(true)
    try {
      const run = await testFlow(draft.id, {
        fields: { email: "deneme@example.invalid", mesaj: t`A test message` },
        form: "deneme",
        title: t`A test message`,
      })
      load()
      if (run.status === "succeeded") {
        toast.success(t`It ran, and every step worked`)
      } else {
        toast.error(run.error ?? t`It ran and something failed`)
      }
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not run it`
      )
    } finally {
      setBusy(false)
    }
  }

  const remove = async (flow: Flow) => {
    await deleteFlow(flow.id).catch(() => undefined)
    if (draft?.id === flow.id) setDraft(null)
    load()
  }

  const move = (from: number, to: number) => {
    if (!draft || to < 0 || to >= draft.steps.length) return
    const steps = [...draft.steps]
    const [held] = steps.splice(from, 1)
    steps.splice(to, 0, held)
    setDraft({ ...draft, steps })
    setChosen({ kind: held.action, index: to })
  }

  const chosenStep = chosen?.index !== undefined ? draft?.steps[chosen.index] : null

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`Flows`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Something happens, so the site does something: a form arrives and somebody is emailed, a post goes out and an address is called.`}
          </p>
        </div>
        <Button onClick={() => { setDraft({ ...BLANK }); setChosen(null) }}>
          <Plus /> {t`New flow`}
        </Button>
      </div>

      {!draft ? (
        <>
          {!flows ? (
            <div className="flex justify-center py-10">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : flows.length === 0 ? (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Workflow />
                </EmptyMedia>
                <EmptyTitle>{t`No flows yet`}</EmptyTitle>
                <EmptyDescription>
                  {t`The first one most sites want: a contact form that emails whoever answers it.`}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
              {flows.map((flow) => (
                <div key={flow.id} className="flex items-center gap-3 px-4 py-3">
                  <Workflow className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium">{flow.name}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {named[flow.trigger_kind] ?? flow.trigger_kind} ·{" "}
                      {flow.steps.length === 1
                        ? t`one step`
                        : t`${flow.steps.length} steps`}
                      {flow.enabled ? "" : ` · ${t`switched off`}`}
                    </p>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setDraft(asDraft(flow)); setChosen(null) }}
                  >
                    {t`Open`}
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t`Delete`}
                    onClick={() => void remove(flow)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))}
            </div>
          )}

          {runs.length > 0 ? (
            <div className="mt-8">
              <h2 className="mb-2 text-base font-semibold">{t`What has been happening`}</h2>
              <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
                {runs.slice(0, 15).map((run) => (
                  <div key={run.id} className="flex items-center gap-3 px-4 py-2">
                    <span
                      className={
                        run.status === "succeeded"
                          ? "size-2 rounded-full bg-emerald-500"
                          : run.status === "failed"
                            ? "size-2 rounded-full bg-destructive"
                            : "size-2 rounded-full bg-muted-foreground"
                      }
                    />
                    <span className="flex-1 truncate text-sm">
                      {flows?.find((f) => f.id === run.flow_id)?.name ?? run.flow_id}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                      {run.error ?? run.status}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {new Date(run.created_at).toLocaleString()}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </>
      ) : (
        <div className="flex flex-col gap-6">
          <div className="flex flex-wrap items-end gap-3">
            <div className="flex min-w-56 flex-1 flex-col gap-2">
              <Label htmlFor="flow-name">{t`Name`}</Label>
              <Input
                id="flow-name"
                value={draft.name}
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                placeholder={t`Contact form to the inbox`}
              />
            </div>
            <div className="flex items-center gap-2 pb-2">
              <Switch
                id="flow-on"
                checked={draft.enabled}
                onCheckedChange={(on) => setDraft({ ...draft, enabled: on })}
              />
              <Label htmlFor="flow-on">{t`On`}</Label>
            </div>
            <Button onClick={() => void save()} disabled={busy || !draft.name.trim()}>
              {busy ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
            <Button variant="outline" onClick={() => void tryIt()} disabled={busy}>
              <Play /> {t`Try it`}
            </Button>
            <Button variant="ghost" onClick={() => { setDraft(null); load() }}>
              {t`Back`}
            </Button>
          </div>

          <div className="grid gap-6 lg:grid-cols-[1fr_22rem]">
            <div className="flex flex-col gap-3">
              <FlowCanvas
                trigger={{ kind: draft.trigger_kind }}
                steps={draft.steps.map((step) => ({ action: step.action }))}
                chosen={chosen}
                onChoose={setChosen}
              />
              <div className="flex flex-wrap gap-2">
                {(vocabulary?.actions ?? []).map((one) => (
                  <Button
                    key={one.kind}
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      const steps = [
                        ...draft.steps,
                        { action: one.kind, config: {}, on_error: "stop" },
                      ]
                      setDraft({ ...draft, steps })
                      setChosen({ kind: one.kind, index: steps.length - 1 })
                    }}
                  >
                    <Plus /> {named[one.kind] ?? one.kind}
                  </Button>
                ))}
              </div>
            </div>

            <div className="flex flex-col gap-4 rounded-xl border border-border p-4">
              {chosen?.index === undefined ? (
                <>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="flow-trigger">{t`What sets it off`}</Label>
                    <Select
                      value={draft.trigger_kind}
                      onValueChange={(kind) =>
                        setDraft({
                          ...draft,
                          trigger_kind: kind ?? "form.submitted",
                          trigger_config: {},
                        })
                      }
                    >
                      <SelectTrigger id="flow-trigger">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {(vocabulary?.triggers ?? []).map((one) => (
                          <SelectItem key={one.kind} value={one.kind}>
                            {named[one.kind] ?? one.kind}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>

                  <TriggerSettings
                    kind={draft.trigger_kind}
                    config={draft.trigger_config}
                    onChange={(config) => setDraft({ ...draft, trigger_config: config })}
                  />

                  {draft.webhook_url ? (
                    <div className="flex flex-col gap-2">
                      <Label>{t`Its address`}</Label>
                      <div className="flex gap-2">
                        <Input readOnly value={draft.webhook_url} />
                        <Button
                          variant="outline"
                          size="icon"
                          aria-label={t`Copy`}
                          onClick={() => {
                            void navigator.clipboard.writeText(draft.webhook_url ?? "")
                            toast.success(t`Copied`)
                          }}
                        >
                          <Copy />
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </>
              ) : chosenStep ? (
                <>
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-medium">
                      {named[chosenStep.action] ?? chosenStep.action}
                    </p>
                    <div className="flex gap-1">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t`Move up`}
                        onClick={() => move(chosen.index!, chosen.index! - 1)}
                      >
                        <ArrowUp />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t`Move down`}
                        onClick={() => move(chosen.index!, chosen.index! + 1)}
                      >
                        <ArrowDown />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={t`Delete`}
                        onClick={() => {
                          const steps = draft.steps.filter((_, i) => i !== chosen.index)
                          setDraft({ ...draft, steps })
                          setChosen(null)
                        }}
                      >
                        <Trash2 />
                      </Button>
                    </div>
                  </div>

                  <StepSettings
                    action={chosenStep.action}
                    config={chosenStep.config}
                    onError={chosenStep.on_error}
                    vocabulary={vocabulary}
                    triggerKind={draft.trigger_kind}
                    onChange={(config) => {
                      const steps = [...draft.steps]
                      steps[chosen.index!] = { ...steps[chosen.index!], config }
                      setDraft({ ...draft, steps })
                    }}
                    onErrorChange={(value) => {
                      const steps = [...draft.steps]
                      steps[chosen.index!] = { ...steps[chosen.index!], on_error: value }
                      setDraft({ ...draft, steps })
                    }}
                  />
                </>
              ) : null}
            </div>
          </div>
        </div>
      )}
    </>
  )
}
