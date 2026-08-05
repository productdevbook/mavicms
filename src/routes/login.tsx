/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { ApiError, login } from "@/lib/api"
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
  component: LoginRoute,
})

function LoginRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { redirect: redirectTo } = Route.useSearch()

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
      await navigate({ to: redirectTo ?? "/editor" })
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
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
