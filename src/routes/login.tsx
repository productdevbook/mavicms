/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { ApiError, getSetupStatus, login } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/login")({
  validateSearch: (
    search: Record<string, unknown>
  ): { redirect?: string } => ({
    redirect: typeof search.redirect === "string" ? search.redirect : undefined,
  }),
  // Whether this address is the server or one of its sites decides what the
  // page offers, and it is the same question the page already asks about
  // whether setup has been done.
  loader: () =>
    getSetupStatus().catch(() => ({
      database_configured: true,
      installed: true,
      site_title: null,
      server: false,
    })),
  component: LoginRoute,
})

/** Sends the browser to a site's own sign-in page. */
function siteLoginUrl(input: string): string | null {
  const host = input
    .trim()
    .toLowerCase()
    .replace(/^https?:\/\//, "")
    .replace(/\/.*$/, "")

  // A hostname and nothing else: this becomes an address the browser is sent
  // to, and anything with a slash or a space in it is not one.
  return /^[a-z0-9.-]+\.[a-z]{2,}$/.test(host)
    ? `https://${host}/admin/login`
    : null
}

function LoginRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { redirect: redirectTo } = Route.useSearch()
  const status = Route.useLoaderData()

  const [site, setSite] = React.useState("")

  const [username, setUsername] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [errorMessage, setErrorMessage] = React.useState("")
  const [submitting, setSubmitting] = React.useState(false)

  const canSubmit = username.trim().length > 0 && password.length > 0 && !submitting

  const submit = async () => {
    setSubmitting(true)
    setErrorMessage("")
    try {
      await login(username.trim(), password)
      await navigate({ to: redirectTo ?? "/dashboard" })
    } catch (error) {
      setErrorMessage(
        error instanceof ApiError
          ? error.message
          : t`Could not reach the server. Please try again.`
      )
      setSubmitting(false)
    }
  }

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-4">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center gap-2">
          <span className="flex size-10 items-center justify-center rounded-xl bg-primary text-lg font-bold text-primary-foreground">
            M
          </span>
          <h1 className="text-lg font-semibold">Mavi CMS</h1>
        </div>

        <Card>
          <CardContent className="pt-6">
            <form
              onSubmit={(event) => {
                event.preventDefault()
                if (canSubmit) void submit()
              }}
              className="flex flex-col gap-4"
            >
              <div>
                <h2 className="text-base font-semibold">
                  <Trans>Sign in</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>Sign in to manage your site.</Trans>
                </p>
              </div>

              {errorMessage && (
                <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {errorMessage}
                </p>
              )}

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="login-username">
                  <Trans>Username</Trans>
                </Label>
                <Input
                  id="login-username"
                  autoFocus
                  value={username}
                  onChange={(event) => setUsername(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="login-password">
                  <Trans>Password</Trans>
                </Label>
                <Input
                  id="login-password"
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
              </div>

              <Button type="submit" disabled={!canSubmit} className="w-full">
                {submitting ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  t`Sign in`
                )}
              </Button>
            </form>

            {/* Only on the server's own address. On a site, this is the only
                sign-in there is and the other doors are none of its business. */}
            {status.server && (
              <div className="mt-6 flex flex-col gap-3 border-t border-border pt-5">
                <p className="text-sm text-muted-foreground">
                  <Trans>
                    This signs you in to the server itself. Everyone else signs
                    in somewhere else:
                  </Trans>
                </p>

                <Link
                  to="/console/login"
                  className="text-sm font-medium hover:underline"
                >
                  <Trans>Agencies → the console</Trans>
                </Link>

                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="login-site">
                    <Trans>Working on a site? Type its address</Trans>
                  </Label>
                  <form
                    className="flex gap-2"
                    onSubmit={(event) => {
                      event.preventDefault()
                      const url = siteLoginUrl(site)
                      if (url) window.location.href = url
                    }}
                  >
                    <Input
                      id="login-site"
                      value={site}
                      onChange={(event) => setSite(event.target.value)}
                      placeholder="example.com"
                    />
                    <Button
                      type="submit"
                      variant="outline"
                      disabled={!siteLoginUrl(site)}
                    >
                      <Trans>Go</Trans>
                    </Button>
                  </form>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
