import * as React from "react"
import {
  Background,
  Controls,
  Handle,
  Position,
  ReactFlow,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react"
import "@xyflow/react/dist/style.css"
import { useLingui } from "@lingui/react/macro"
import { Globe, GitBranch, Mails, MessageCircle, Play, Send } from "lucide-react"

import { cn } from "@/lib/utils"

export type Piece = {
  kind: string
  /** Filled in for a step; absent for the trigger. */
  index?: number
}

const LOOK: Record<string, { icon: typeof Play; tone: string }> = {
  "form.submitted": { icon: Play, tone: "bg-emerald-500" },
  "post.published": { icon: Play, tone: "bg-emerald-500" },
  schedule: { icon: Play, tone: "bg-emerald-500" },
  webhook: { icon: Play, tone: "bg-emerald-500" },
  "mail.send": { icon: Mails, tone: "bg-blue-500" },
  "http.request": { icon: Globe, tone: "bg-violet-500" },
  branch: { icon: GitBranch, tone: "bg-amber-500" },
  "slack.message": { icon: MessageCircle, tone: "bg-rose-500" },
  "discord.message": { icon: MessageCircle, tone: "bg-indigo-500" },
  "telegram.message": { icon: Send, tone: "bg-sky-500" },
}

/**
 * One box on the canvas.
 *
 * Deliberately not draggable-to-connect: a flow here is a list, in order, and
 * a canvas that lets somebody draw a shape the server cannot run is a canvas
 * that lies. The picture is for reading; the order is changed with the arrows
 * beside each step.
 */
function Box({ data, selected }: NodeProps) {
  const piece = data as { label: string; kind: string; note?: string }
  const look = LOOK[piece.kind] ?? { icon: Globe, tone: "bg-slate-500" }
  const Icon = look.icon

  return (
    <div
      className={cn(
        "w-56 rounded-xl border bg-card px-3 py-2 shadow-sm transition",
        selected ? "border-primary ring-2 ring-primary/30" : "border-border"
      )}
    >
      <Handle type="target" position={Position.Top} className="!bg-border" />
      <div className="flex items-center gap-2">
        <span
          className={cn(
            "flex size-6 shrink-0 items-center justify-center rounded-md text-white",
            look.tone
          )}
        >
          <Icon className="size-3.5" />
        </span>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{piece.label}</p>
          {piece.note ? (
            <p className="truncate text-xs text-muted-foreground">{piece.note}</p>
          ) : null}
        </div>
      </div>
      <Handle type="source" position={Position.Bottom} className="!bg-border" />
    </div>
  )
}

const TYPES = { box: Box }

export function FlowCanvas({
  trigger,
  steps,
  chosen,
  onChoose,
}: {
  trigger: { kind: string; note?: string }
  steps: { action: string; note?: string }[]
  chosen: Piece | null
  onChoose: (piece: Piece) => void
}) {
  const { t } = useLingui()

  const named: Record<string, string> = {
    "form.submitted": t`A form is filled in`,
    "post.published": t`Something is published`,
    schedule: t`Every so often`,
    webhook: t`An address is called`,
    "mail.send": t`Send an email`,
    "http.request": t`Call an address`,
    branch: t`Only carry on if`,
    "slack.message": t`Post to Slack`,
    "discord.message": t`Post to Discord`,
    "telegram.message": t`Send on Telegram`,
  }

  const nodes: Node[] = React.useMemo(() => {
    const all: Node[] = [
      {
        id: "trigger",
        type: "box",
        position: { x: 0, y: 0 },
        selected: chosen?.index === undefined,
        data: {
          label: named[trigger.kind] ?? trigger.kind,
          kind: trigger.kind,
          note: trigger.note,
        },
      },
    ]
    steps.forEach((step, index) => {
      all.push({
        id: `step-${index}`,
        type: "box",
        position: { x: 0, y: (index + 1) * 110 },
        selected: chosen?.index === index,
        data: {
          label: named[step.action] ?? step.action,
          kind: step.action,
          note: step.note,
        },
      })
    })
    return all
    // eslint-disable-next-line react-hooks/exhaustive-deps -- named is rebuilt every render
  }, [trigger, steps, chosen])

  const edges: Edge[] = React.useMemo(
    () =>
      steps.map((_, index) => ({
        id: `e-${index}`,
        source: index === 0 ? "trigger" : `step-${index - 1}`,
        target: `step-${index}`,
        animated: true,
      })),
    [steps]
  )

  return (
    <div className="h-[420px] w-full overflow-hidden rounded-xl border border-border">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={TYPES}
        fitView
        proOptions={{ hideAttribution: false }}
        nodesDraggable={false}
        nodesConnectable={false}
        edgesFocusable={false}
        onNodeClick={(_, node) =>
          onChoose(
            node.id === "trigger"
              ? { kind: trigger.kind }
              : { kind: steps[Number(node.id.slice(5))].action, index: Number(node.id.slice(5)) }
          )
        }
      >
        <Background />
        <Controls showInteractive={false} />
      </ReactFlow>
    </div>
  )
}
