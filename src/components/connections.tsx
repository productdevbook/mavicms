import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Bot, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  disconnect,
  listConnections,
  type Connection,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

/**
 * The assistants somebody has connected to this site.
 *
 * The list matters more than it looks: a connection is made in a dialog
 * belonging to somebody else's program, and this is the only place it can be
 * seen from the site's own side — and the only place it can be ended.
 */
export function Connections() {
  const { t } = useLingui()
  const [connected, setConnected] = React.useState<Connection[] | null>(null)

  const load = React.useCallback(() => {
    listConnections()
      .then(setConnected)
      .catch(() => toast.error(t`Could not load the connections`))
  }, [t])

  React.useEffect(load, [load])

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

  return (
    <section className="flex flex-col gap-3">
      <div>
        <h2 className="text-sm font-medium">{t`Connected assistants`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Programs that have been through this site's sign-in and been allowed in. Disconnecting one stops it immediately.`}
        </p>
      </div>

      {connected === null ? (
        <div className="flex justify-center py-8">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
        </div>
      ) : connected.length === 0 ? (
        <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
          {t`Nothing is connected`}
        </p>
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
    </section>
  )
}
