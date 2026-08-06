import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { KeyRound, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createConsoleToken,
  deleteConsoleToken,
  listConsoleTokens,
  type DevelopmentToken,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { McpConnection } from "@/components/mcp-connection"

/**
 * The credential an agency hands to an assistant.
 *
 * Not the session this browser is signed in with: that one is a person being
 * logged in, and a list of tokens that showed it — or a revoke that took it
 * away — would be a colleague signed out with no explanation.
 */
export function ConsoleTokens() {
  const { t } = useLingui()

  const [tokens, setTokens] = React.useState<DevelopmentToken[] | null>(null)
  // Held until the page is left. The server keeps no copy to show again.
  const [fresh, setFresh] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)

  const load = React.useCallback(() => {
    listConsoleTokens()
      .then(setTokens)
      .catch(() => toast.error(t`Could not load the tokens`))
  }, [t])

  React.useEffect(load, [load])

  const mint = async () => {
    setBusy(true)
    try {
      const { token } = await createConsoleToken()
      setFresh(token)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not make a token`
      )
    } finally {
      setBusy(false)
    }
  }

  const revoke = async (id: string) => {
    try {
      await deleteConsoleToken(id)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not take it back`
      )
    }
  }

  return (
    <div className="flex max-w-3xl flex-col gap-8">
      <McpConnection origin={window.location.origin} name="mavicms" token={fresh}>
        <p className="text-sm text-muted-foreground">
          {t`From here it answers about every site this agency looks after: which of them built, which did not, what a failing build said, and adding a new one. Writing a post is a question for that site's own address.`}
        </p>
      </McpConnection>

      <section className="flex flex-col gap-3">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-sm font-medium">{t`Tokens`}</h2>
            <p className="text-sm text-muted-foreground">
              {t`One per assistant. It acts for this agency, lasts 90 days, and can be taken back here. It is not a way into any site's content.`}
            </p>
          </div>
          <Button onClick={() => void mint()} disabled={busy}>
            {busy ? <Loader2 className="animate-spin" /> : <KeyRound />}
            {t`Make a token`}
          </Button>
        </div>

        {fresh && (
          <div className="rounded-xl border border-border px-4 py-3">
            <p className="mb-1 text-sm font-medium">{t`Copy it now`}</p>
            <p className="mb-2 text-sm text-muted-foreground">
              {t`This is the only time it is shown. Leave this page and it cannot be read again — make another one instead.`}
            </p>
            <code className="block overflow-x-auto font-mono text-xs">
              {fresh}
            </code>
          </div>
        )}

        {tokens === null ? (
          <div className="flex justify-center py-8">
            <Loader2 className="size-5 animate-spin text-muted-foreground" />
          </div>
        ) : tokens.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`No tokens yet`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {tokens.map((token) => (
              <div key={token.id} className="flex items-center gap-3 px-4 py-2.5">
                <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="text-sm">
                    {t`Made ${new Date(token.created_at).toLocaleString()}`}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {t`Stops working ${new Date(token.expires_at).toLocaleString()}`}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Take it back`}
                  onClick={() => void revoke(token.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
