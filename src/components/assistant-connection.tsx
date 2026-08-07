import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import {
  Bot,
  Check,
  Copy,
  Eye,
  Loader2,
  Pencil,
  Trash2,
  TriangleAlert,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  disconnect,
  listAssistantTools,
  listConnections,
  type AssistantTool,
  type Connection,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

/**
 * One page for one job: connecting an assistant to this site.
 *
 * It used to be three sections in three places — how to connect, what is
 * connected, and a token — with the reference documentation for building a
 * front end in between them. Somebody opening "Connected assistants" saw an
 * empty list and no way to make it not empty.
 */

function Copyable({ text, label }: { text: string; label: string }) {
  const { t } = useLingui()
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-lg border border-border bg-muted/40 px-3 py-2.5 pr-10 text-sm">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        className="absolute right-1 top-1"
        aria-label={label}
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

function Step({
  number,
  title,
  children,
}: {
  number: number
  title: string
  children: React.ReactNode
}) {
  return (
    <div className="flex gap-3">
      <div className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium">
        {number}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-2 pb-2">
        <h3 className="text-sm font-medium">{title}</h3>
        {children}
      </div>
    </div>
  )
}

export function AssistantConnection({ origin }: { origin: string }) {
  const { t } = useLingui()
  const url = `${origin}/api/mcp`

  const [tools, setTools] = React.useState<AssistantTool[] | null>(null)
  const [connected, setConnected] = React.useState<Connection[] | null>(null)

  const load = React.useCallback(() => {
    listConnections()
      .then(setConnected)
      .catch(() => toast.error(t`Could not load the connections`))
  }, [t])

  React.useEffect(load, [load])

  React.useEffect(() => {
    // A failure here is not worth a message: the list below it is the part
    // somebody came for, and this is an explanation of it.
    listAssistantTools()
      .then(setTools)
      .catch(() => setTools([]))
  }, [])

  const end = async (id: string) => {
    try {
      await disconnect(id)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not disconnect it`
      )
    }
  }

  const reading = tools?.filter((tool) => tool.reads) ?? []
  const changing = tools?.filter((tool) => !tool.reads) ?? []

  return (
    <div className="flex flex-col gap-8">
      <div>
        <h2 className="text-sm font-medium">{t`Connecting an assistant`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`An assistant pointed at this site can be asked to do the work rather than told how to do it — find a post and correct it, tell you what came in through the forms this week, put a piece online.`}
        </p>
      </div>

      <div className="flex flex-col gap-4">
        <Step number={1} title={t`Give it this address`}>
          <Copyable text={url} label={t`Copy the address`} />
          <p className="text-sm text-muted-foreground">
            {t`Wherever the program asks for an MCP server, or a custom connector, paste this and nothing else. There is no token to find and no file to edit.`}
          </p>
        </Step>

        <Step number={2} title={t`Sign in, here`}>
          <p className="text-sm text-muted-foreground">
            {t`It will send you to this site to sign in and ask whether you want to allow it. Nothing is connected until you say yes, and the program never sees your password or anything you could paste elsewhere.`}
          </p>
        </Step>

        <Step number={3} title={t`It can then do what you can`}>
          {tools === null ? (
            <Loader2 className="size-4 animate-spin text-muted-foreground" />
          ) : tools.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {t`This site's tools could not be listed just now.`}
            </p>
          ) : (
            <div className="flex flex-col gap-3">
              <ToolGroup
                icon={<Pencil className="size-3.5" />}
                title={t`Changes things`}
                tools={changing}
              />
              <ToolGroup
                icon={<Eye className="size-3.5" />}
                title={t`Only reads`}
                tools={reading}
              />
              <p className="text-sm text-muted-foreground">
                {t`Nothing else. There is no tool here that deletes a post or a file — that stays something a person does in this panel.`}
              </p>
            </div>
          )}
        </Step>
      </div>

      <section className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">{t`Connected now`}</h2>

        {connected === null ? (
          <div className="flex justify-center py-8">
            <Loader2 className="size-5 animate-spin text-muted-foreground" />
          </div>
        ) : connected.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border px-6 py-8 text-center">
            <Bot className="mx-auto mb-2 size-5 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">
              {t`Nothing is connected yet. Follow the three steps above and whatever you allowed will appear here.`}
            </p>
          </div>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {connected.map((connection) => (
              <div
                key={connection.id}
                className="flex items-center gap-3 px-4 py-2.5"
              >
                <Bot className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm">{connection.client_name}</p>
                  <p className="text-xs text-muted-foreground">
                    {t`As ${connection.username}, since ${new Date(connection.created_at).toLocaleDateString()}`}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Disconnect`}
                  onClick={() => void end(connection.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}

        {connected !== null && connected.length > 0 && (
          <p className="text-sm text-muted-foreground">
            {t`Disconnecting one stops it that moment.`}
          </p>
        )}
      </section>
    </div>
  )
}

function ToolGroup({
  icon,
  title,
  tools,
}: {
  icon: React.ReactNode
  title: string
  tools: AssistantTool[]
}) {
  if (tools.length === 0) return null

  return (
    <div className="rounded-xl border border-border">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2 text-xs font-medium text-muted-foreground">
        {icon}
        {title}
      </div>
      <ul className="divide-y divide-border">
        {tools.map((tool) => (
          <li key={tool.name} className="flex items-start gap-2 px-3 py-2">
            {tool.destroys && (
              <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
            )}
            <span className="text-sm">{tool.title}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}
