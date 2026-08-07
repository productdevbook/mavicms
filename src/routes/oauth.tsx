/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Bot, Loader2, ShieldQuestion } from "lucide-react"

import {
  ApiError,
  approveConnection,
  describeConnectionRequest,
  type ConnectionRequest,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/oauth")({
  component: ConsentRoute,
})

/**
 * The screen somebody sees when a program asks to connect to their site.
 *
 * It is here rather than on the server so that it looks like the site it is
 * asking about — a consent screen that looks like nothing in particular is one
 * where "this is a phishing page" and "this is the real thing" are the same
 * picture. The program's name is its own claim about itself, and the screen
 * says so rather than presenting it as established.
 */
function ConsentRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [asking, setAsking] = React.useState<ConnectionRequest | null>(null)
  const [refusal, setRefusal] = React.useState<string | null>(null)
  const [allowing, setAllowing] = React.useState(false)

  const query = React.useMemo(() => window.location.search, [])

  React.useEffect(() => {
    describeConnectionRequest(query)
      .then(setAsking)
      .catch((error: unknown) => {
        if (error instanceof ApiError && error.status === 401) {
          // Sign in first, and come back to exactly this question.
          void navigate({
            to: "/login",
            search: { redirect: `/oauth${query}` },
          })
          return
        }
        setRefusal(
          error instanceof ApiError
            ? error.message
            : t`That request could not be read`
        )
      })
  }, [query, navigate, t])

  const allow = async () => {
    setAllowing(true)
    try {
      const { redirect } = await approveConnection(query)
      // Out of the panel and back to whoever asked, with the code.
      window.location.replace(redirect)
    } catch (error) {
      setAllowing(false)
      setRefusal(
        error instanceof ApiError ? error.message : t`It could not be allowed`
      )
    }
  }

  if (refusal) {
    return (
      <Shell>
        <ShieldQuestion className="size-8 text-muted-foreground" />
        <h1 className="text-lg font-semibold">{t`This cannot be connected`}</h1>
        <p className="text-sm text-muted-foreground">{refusal}</p>
        <p className="text-sm text-muted-foreground">
          {t`Nothing has been given to anybody. You can close this page.`}
        </p>
      </Shell>
    )
  }

  if (!asking) {
    return (
      <Shell>
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </Shell>
    )
  }

  return (
    <Shell>
      <Bot className="size-8 text-muted-foreground" />

      <h1 className="text-lg font-semibold">
        {t`Connect ${asking.client_name} to ${asking.site_title}?`}
      </h1>

      <p className="text-sm text-muted-foreground">
        {t`It will be able to do what you can: read this site's posts, write and change them, see what has come in through the forms, and ask for the pages to be built. It cannot delete anything.`}
      </p>

      <div className="w-full rounded-xl border border-border px-4 py-3 text-left">
        <p className="text-sm">
          {t`Connecting as ${asking.username}`}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {t`It calls itself "${asking.client_name}", which is its own word for itself and not something this site can check. If you did not just ask a program to connect, close this page.`}
        </p>
        <p className="mt-2 break-all font-mono text-xs text-muted-foreground">
          {asking.redirect_uri}
        </p>
      </div>

      <div className="flex w-full gap-2">
        <Button
          variant="outline"
          className="flex-1"
          onClick={() => window.close()}
        >
          {t`No`}
        </Button>
        <Button
          className="flex-1"
          onClick={() => void allow()}
          disabled={allowing}
        >
          {allowing ? <Loader2 className="animate-spin" /> : null}
          {t`Connect`}
        </Button>
      </div>

      <p className="text-xs text-muted-foreground">
        {t`You can disconnect it later from API in the panel.`}
      </p>
    </Shell>
  )
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-svh items-center justify-center px-6 py-12">
      <div className="flex w-full max-w-md flex-col items-center gap-4 text-center">
        {children}
      </div>
    </div>
  )
}
