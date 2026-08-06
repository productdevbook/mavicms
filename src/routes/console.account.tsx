/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { ArrowLeft, Loader2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  getConsoleAccount,
  updateConsoleAccount,
  type ConsoleAccount,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

export const Route = createFileRoute("/console/account")({
  beforeLoad: async () => {
    const account = await getConsoleAccount().catch(() => {
      throw redirect({ to: "/console/login" })
    })
    return { account }
  },
  component: ConsoleAccountRoute,
})

const MINIMUM_PASSWORD = 10

function ConsoleAccountRoute() {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { account } = Route.useRouteContext() as { account: ConsoleAccount }

  const [name, setName] = React.useState(account.name)
  const [email, setEmail] = React.useState(account.email)
  const [current, setCurrent] = React.useState("")
  const [next, setNext] = React.useState("")
  const [saving, setSaving] = React.useState(false)

  const save = async () => {
    setSaving(true)
    try {
      await updateConsoleAccount({
        current_password: current,
        name: name.trim(),
        email: email.trim(),
        // Left out unless typed: saving the form is not changing the password.
        ...(next ? { new_password: next } : {}),
      })
      setCurrent("")
      setNext("")
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the settings`
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="min-h-svh bg-background">
      <main className="mx-auto w-full max-w-2xl px-6 py-8">
        <Button
          variant="ghost"
          size="sm"
          className="mb-4 -ml-2"
          onClick={() => void navigate({ to: "/console" })}
        >
          <ArrowLeft /> {t`Your sites`}
        </Button>

        <div className="mb-6">
          <h1 className="text-lg font-semibold">{t`Your account`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`${account.organization_name}. This is the account that opens and manages your sites.`}
          </p>
        </div>

        <div className="flex flex-col gap-6">
          <div className="flex flex-col gap-2">
            <Label htmlFor="account-name">{t`Your name`}</Label>
            <Input
              id="account-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="account-email">{t`Email`}</Label>
            <Input
              id="account-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
            <p className="text-sm text-muted-foreground">
              {t`This is what you sign in with.`}
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="account-next">{t`New password`}</Label>
            <Input
              id="account-next"
              type="password"
              value={next}
              onChange={(event) => setNext(event.target.value)}
              placeholder={t`Leave empty to keep the one you have`}
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="account-current">{t`Current password`}</Label>
            <Input
              id="account-current"
              type="password"
              value={current}
              onChange={(event) => setCurrent(event.target.value)}
            />
            <p className="text-sm text-muted-foreground">
              {t`Asked for whatever you are changing — a session left open somewhere should not be enough to take the account away.`}
            </p>
          </div>

          <div>
            <Button
              onClick={() => void save()}
              disabled={
                saving ||
                !current ||
                (next.length > 0 && next.length < MINIMUM_PASSWORD)
              }
            >
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
          </div>
        </div>
      </main>
    </div>
  )
}
