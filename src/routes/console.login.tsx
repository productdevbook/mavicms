/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { ApiError, consoleLogin } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/console/login")({
  component: ConsoleLoginRoute,
})

function ConsoleLoginRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [errorMessage, setErrorMessage] = React.useState("")
  const [submitting, setSubmitting] = React.useState(false)

  const canSubmit = email.trim().length > 0 && password.length > 0 && !submitting

  const submit = async () => {
    setSubmitting(true)
    setErrorMessage("")
    try {
      await consoleLogin(email.trim(), password)
      await navigate({ to: "/console" })
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
                  <Trans>Agency sign in</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>Manage the sites your agency runs.</Trans>
                </p>
              </div>

              {errorMessage && (
                <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {errorMessage}
                </p>
              )}

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="console-email">
                  <Trans>Email</Trans>
                </Label>
                <Input
                  id="console-email"
                  type="email"
                  autoFocus
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="console-password">
                  <Trans>Password</Trans>
                </Label>
                <Input
                  id="console-password"
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

              <p className="text-center text-sm text-muted-foreground">
                <Link to="/console/register" className="hover:underline">
                  <Trans>Open an agency account</Trans>
                </Link>
              </p>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
