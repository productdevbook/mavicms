/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"

import { ApiError, consoleRegister } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/console/register")({
  component: ConsoleRegisterRoute,
})

const MINIMUM_PASSWORD = 10

function ConsoleRegisterRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()

  const [organizationName, setOrganizationName] = React.useState("")
  const [name, setName] = React.useState("")
  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [errorMessage, setErrorMessage] = React.useState("")
  const [submitting, setSubmitting] = React.useState(false)

  const canSubmit =
    organizationName.trim().length > 0 &&
    email.trim().includes("@") &&
    password.length >= MINIMUM_PASSWORD &&
    !submitting

  const submit = async () => {
    setSubmitting(true)
    setErrorMessage("")
    try {
      await consoleRegister({
        organization_name: organizationName.trim(),
        name: name.trim(),
        email: email.trim(),
        password,
      })
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
                  <Trans>Open an agency account</Trans>
                </h2>
                <p className="text-sm text-muted-foreground">
                  <Trans>
                    One account for every site you run. Each site keeps its own
                    content and its own people.
                  </Trans>
                </p>
              </div>

              {errorMessage && (
                <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
                  {errorMessage}
                </p>
              )}

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="register-organization">
                  <Trans>Agency name</Trans>
                </Label>
                <Input
                  id="register-organization"
                  autoFocus
                  value={organizationName}
                  onChange={(event) => setOrganizationName(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="register-name">
                  <Trans>Your name</Trans>
                </Label>
                <Input
                  id="register-name"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="register-email">
                  <Trans>Email</Trans>
                </Label>
                <Input
                  id="register-email"
                  type="email"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="register-password">
                  <Trans>Password</Trans>
                </Label>
                <Input
                  id="register-password"
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  {t`At least ${MINIMUM_PASSWORD} characters.`}
                </p>
              </div>

              <Button type="submit" disabled={!canSubmit} className="w-full">
                {submitting ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  t`Create the account`
                )}
              </Button>

              <p className="text-center text-sm text-muted-foreground">
                <Link to="/console/login" className="hover:underline">
                  <Trans>Already have an account? Sign in</Trans>
                </Link>
              </p>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
