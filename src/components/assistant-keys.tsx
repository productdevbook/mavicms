import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { KeyRound, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  deleteAssistantKey,
  listAssistantKeys,
  type AssistantKey,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

/**
 * The keys handed out from the section above, and the way to take one back.
 *
 * Kept separate from making them: making one is part of copying the document,
 * and what somebody wants from a list is to see how many are out and end the
 * one they pasted into the wrong window.
 */
export function AssistantKeys({ reloadOn }: { reloadOn: number }) {
  const { t, i18n } = useLingui()
  const [keys, setKeys] = React.useState<AssistantKey[] | null>(null)

  const load = React.useCallback(() => {
    listAssistantKeys()
      .then(setKeys)
      .catch(() => toast.error(t`Could not load the keys`))
  }, [t])

  React.useEffect(load, [load, reloadOn])

  const revoke = async (id: string) => {
    try {
      await deleteAssistantKey(id)
      load()
      toast.success(t`That key stopped working`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not take it back`
      )
    }
  }

  const when = (iso: string) =>
    new Date(iso).toLocaleString(i18n.locale, {
      dateStyle: "medium",
      timeStyle: "short",
    })

  if (keys === null) {
    return (
      <div className="flex justify-center py-6">
        <Loader2 className="size-5 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (keys.length === 0) return null

  return (
    <div className="flex flex-col gap-2">
      <h3 className="text-sm font-medium">{t`Keys you have handed out`}</h3>
      <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
        {keys.map((key) => {
          const over = new Date(key.expires_at) <= new Date()
          return (
            <div
              key={key.id}
              className="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2.5"
            >
              <KeyRound className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1 basis-40">
                <p className="text-sm">{t`Handed over ${when(key.created_at)}`}</p>
                <p className="text-xs text-muted-foreground">
                  {over
                    ? t`Ran out ${when(key.expires_at)}`
                    : t`Stops working ${when(key.expires_at)}`}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                className="ml-auto"
                aria-label={t`Take it back`}
                onClick={() => void revoke(key.id)}
              >
                <Trash2 />
              </Button>
            </div>
          )
        })}
      </div>
    </div>
  )
}
