import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Check, Copy, KeyRound, Loader2, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createSiteDevelopmentToken,
  deleteSiteDevelopmentToken,
  getSiteDevelopment,
  type SiteDevelopment,
} from "@/lib/api"
import { Button } from "@/components/ui/button"

/**
 * The `.env` somebody needs to run a site's front end on their own machine.
 *
 * This exists because the question kept being asked by message. A designer
 * clones the project, runs it, sees no posts, and writes to the agency to ask
 * what to put in the file — and the agency answers with a password, over chat,
 * where it stays. So: the file is written here, and the credential in it is a
 * read-only token that can be taken back.
 */

/** Where a real value would go, so a half-filled file is obviously half-filled. */
const BLANK = "…"

function envFile(development: SiteDevelopment, token: string | null): string {
  const lines = [
    `CMS_API_URL=${development.api_url}`,
    `SITE_URL=${development.site_url}`,
    `CMS_TOKEN=${token ?? BLANK}`,
  ]

  if (development.variables.length > 0) {
    lines.push(
      "",
      "# This site's build also runs with these. Their values are kept",
      "# encrypted on the server and are not shown, here or anywhere.",
      ...development.variables.map((name) => `${name}=${BLANK}`)
    )
  }

  return lines.join("\n") + "\n"
}

export function LocalEnv({ siteId }: { siteId: string }) {
  const { t } = useLingui()

  const [development, setDevelopment] = React.useState<SiteDevelopment | null>(
    null
  )
  // Held only until the page is left. The server keeps no copy it could show
  // again, which is the point of it.
  const [fresh, setFresh] = React.useState<string | null>(null)
  const [busy, setBusy] = React.useState(false)
  const [copied, setCopied] = React.useState(false)

  const load = React.useCallback(() => {
    getSiteDevelopment(siteId)
      .then(setDevelopment)
      .catch(() => toast.error(t`Could not load the local settings`))
  }, [siteId, t])

  React.useEffect(load, [load])

  const mint = async () => {
    setBusy(true)
    try {
      const { token } = await createSiteDevelopmentToken(siteId)
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

  const revoke = async (tokenId: string) => {
    try {
      await deleteSiteDevelopmentToken(siteId, tokenId)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not take it back`
      )
    }
  }

  if (!development) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  const file = envFile(development, fresh)

  const copy = () => {
    void navigator.clipboard.writeText(file).then(
      () => {
        setCopied(true)
        setTimeout(() => setCopied(false), 1500)
      },
      () => toast.error(t`Could not copy it`)
    )
  }

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t`Working on this site locally`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Put this in a .env beside the project. It points at the live site, so what a designer sees on their machine is what is actually written here.`}
        </p>

        <div className="relative">
          <pre className="overflow-x-auto rounded-xl border border-border bg-muted/40 px-4 py-3 pr-12 text-xs">
            {file}
          </pre>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t`Copy`}
            className="absolute top-2 right-2"
            onClick={copy}
          >
            {copied ? <Check /> : <Copy />}
          </Button>
        </div>

        {!fresh && (
          <p className="text-sm text-muted-foreground">
            {t`CMS_TOKEN is blank until you make one. Without it the site answers nothing, which is the "why can I not see any posts" everybody runs into.`}
          </p>
        )}
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 className="text-sm font-medium">{t`Tokens`}</h2>
            <p className="text-sm text-muted-foreground">
              {t`One per person. It can read this site and change nothing, lasts 30 days, and can be taken back here.`}
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

        {development.tokens.length === 0 ? (
          <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
            {t`No tokens yet`}
          </p>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {development.tokens.map((token) => (
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
      </div>
    </div>
  )
}
