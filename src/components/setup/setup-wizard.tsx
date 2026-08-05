import * as React from "react"
import { Trans, useLingui } from "@lingui/react/macro"
import {
  AlertTriangle,
  Check,
  Eye,
  EyeOff,
  Loader2,
  Sparkles,
} from "lucide-react"

import { cn } from "@/lib/utils"
import {
  ApiError,
  configureDatabase,
  getSetupStatus,
  submitSetup,
  type DatabaseEngine,
  type SetupResult,
} from "@/lib/api"
import { generatePassword, passwordStrength } from "@/lib/password"
import { defaultLocale, locales, setLocale, type Locale } from "@/i18n"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

type Step =
  | "language"
  | "database"
  | "site"
  | "account"
  | "installing"
  | "success"
  | "error"

const FORM_STEPS: Step[] = ["language", "database", "site", "account"]

interface FormState {
  locale: Locale
  siteTitle: string
  tagline: string
  adminUsername: string
  adminEmail: string
  adminPassword: string
  confirmWeak: boolean
}

const INITIAL_FORM: FormState = {
  locale: defaultLocale,
  siteTitle: "",
  tagline: "",
  adminUsername: "",
  adminEmail: "",
  adminPassword: "",
  confirmWeak: false,
}

export function SetupWizard({
  onComplete,
  initialDatabaseConfigured,
}: {
  onComplete: () => void
  initialDatabaseConfigured: boolean
}) {
  const { t } = useLingui()
  const [step, setStep] = React.useState<Step>("language")
  const [databaseConfigured, setDatabaseConfigured] = React.useState(
    initialDatabaseConfigured
  )
  const [form, setForm] = React.useState<FormState>(INITIAL_FORM)
  const [result, setResult] = React.useState<SetupResult | null>(null)
  const [errorMessage, setErrorMessage] = React.useState("")

  const patch = (fields: Partial<FormState>) =>
    setForm((current) => ({ ...current, ...fields }))

  const install = async () => {
    setStep("installing")
    try {
      const response = await submitSetup({
        site_title: form.siteTitle.trim(),
        tagline: form.tagline.trim(),
        locale: form.locale,
        admin_username: form.adminUsername.trim(),
        admin_email: form.adminEmail.trim(),
        admin_password: form.adminPassword,
      })
      setResult(response)
      setStep("success")
    } catch (error) {
      setErrorMessage(
        error instanceof ApiError
          ? error.message
          : t`Could not reach the server. Please try again.`
      )
      setStep("error")
    }
  }

  const showProgress = FORM_STEPS.includes(step)
  const stepIndex = FORM_STEPS.indexOf(step)

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-4">
      <div className="w-full max-w-md">
        <div className="mb-6 flex flex-col items-center gap-2">
          <span className="flex size-10 items-center justify-center rounded-xl bg-primary text-lg font-bold text-primary-foreground">
            M
          </span>
          <h1 className="text-lg font-semibold">Mavi CMS</h1>
        </div>

        {showProgress && (
          <div className="mb-4 flex justify-center gap-1.5">
            {FORM_STEPS.map((s, index) => (
              <span
                key={s}
                className={cn(
                  "h-1.5 w-10 rounded-full transition-colors",
                  index <= stepIndex ? "bg-primary" : "bg-border"
                )}
              />
            ))}
          </div>
        )}

        <Card>
          <CardContent className="pt-6">
            {step === "language" && (
              <LanguageStep
                value={form.locale}
                onNext={(locale) => {
                  patch({ locale })
                  setLocale(locale)
                  setStep(databaseConfigured ? "site" : "database")
                }}
              />
            )}
            {step === "database" && (
              <DatabaseStep
                onBack={() => setStep("language")}
                onConfigured={() => {
                  setDatabaseConfigured(true)
                  setStep("site")
                }}
              />
            )}
            {step === "site" && (
              <SiteStep
                siteTitle={form.siteTitle}
                tagline={form.tagline}
                onChange={patch}
                onBack={() =>
                  setStep(initialDatabaseConfigured ? "language" : "database")
                }
                onNext={() => setStep("account")}
              />
            )}
            {step === "account" && (
              <AccountStep
                form={form}
                onChange={patch}
                onBack={() => setStep("site")}
                onSubmit={install}
              />
            )}
            {step === "installing" && <InstallingStep />}
            {step === "success" && result && (
              <SuccessStep result={result} onComplete={onComplete} />
            )}
            {step === "error" && (
              <ErrorStep
                message={errorMessage}
                onRetry={() => setStep("account")}
              />
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

function LanguageStep({
  value,
  onNext,
}: {
  value: Locale
  onNext: (locale: Locale) => void
}) {
  const { t } = useLingui()
  const [selected, setSelected] = React.useState(value)

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Select your language</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">
          <Trans>You can change this later.</Trans>
        </p>
      </div>
      <div className="grid grid-cols-2 gap-2">
        {Object.entries(locales).map(([code, label]) => (
          <button
            key={code}
            type="button"
            onClick={() => setSelected(code as Locale)}
            className={cn(
              "flex items-center justify-between rounded-lg border px-3 py-2.5 text-sm font-medium transition-colors",
              selected === code
                ? "border-primary bg-primary/5 text-primary"
                : "border-border hover:bg-muted"
            )}
          >
            {label}
            {selected === code && <Check className="size-4" />}
          </button>
        ))}
      </div>
      <Button onClick={() => onNext(selected)} className="w-full">
        {t`Continue`}
      </Button>
    </div>
  )
}

const ENGINE_DEFAULT_PORT: Record<DatabaseEngine, number | null> = {
  postgres: 5432,
  mysql: 3306,
  sqlite: null,
}

interface DatabaseFormState {
  mode: "manual" | "url"
  url: string
  engine: DatabaseEngine
  host: string
  port: string
  database: string
  username: string
  password: string
}

const INITIAL_DATABASE_FORM: DatabaseFormState = {
  mode: "manual",
  url: "",
  engine: "postgres",
  host: "",
  port: "",
  database: "",
  username: "",
  password: "",
}

function parseConnectionUrl(raw: string) {
  try {
    const url = new URL(raw)
    return {
      engine: url.protocol.replace(":", ""),
      host: url.hostname || null,
      database: url.pathname.replace(/^\//, "") || null,
      username: url.username || null,
    }
  } catch {
    return null
  }
}

function DatabaseStep({
  onBack,
  onConfigured,
}: {
  onBack: () => void
  onConfigured: () => void
}) {
  const { t } = useLingui()
  const [form, setForm] = React.useState<DatabaseFormState>(INITIAL_DATABASE_FORM)
  const [phase, setPhase] = React.useState<"idle" | "testing" | "restarting" | "error">(
    "idle"
  )
  const [errorMessage, setErrorMessage] = React.useState("")

  const patch = (fields: Partial<DatabaseFormState>) =>
    setForm((current) => ({ ...current, ...fields }))

  const preview = form.mode === "url" ? parseConnectionUrl(form.url) : null

  const canSubmit =
    form.mode === "url"
      ? form.url.trim().length > 0
      : form.engine === "sqlite"
        ? form.database.trim().length > 0
        : form.host.trim().length > 0 &&
          form.database.trim().length > 0 &&
          form.username.trim().length > 0

  const waitForRestart = async () => {
    for (let attempt = 0; attempt < 60; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 1000))
      try {
        const status = await getSetupStatus()
        if (status.database_configured) {
          onConfigured()
          return
        }
      } catch {
        // still restarting — keep polling
      }
    }
    setErrorMessage(
      t`The server is taking longer than expected to restart. Reload this page in a moment.`
    )
    setPhase("error")
  }

  const submit = async () => {
    setPhase("testing")
    try {
      await configureDatabase(
        form.mode === "url"
          ? { url: form.url.trim() }
          : {
              engine: form.engine,
              host: form.host.trim(),
              port: form.port ? Number(form.port) : undefined,
              database: form.database.trim(),
              username: form.username.trim(),
              password: form.password,
            }
      )
      setPhase("restarting")
      void waitForRestart()
    } catch (error) {
      setErrorMessage(
        error instanceof ApiError
          ? error.message
          : t`Could not reach the server. Please try again.`
      )
      setPhase("error")
    }
  }

  if (phase === "testing" || phase === "restarting") {
    return (
      <div className="flex flex-col items-center gap-3 py-8 text-center">
        <Loader2 className="size-6 animate-spin text-primary" />
        <p className="text-sm font-medium">
          {phase === "testing" ? (
            <Trans>Testing connection…</Trans>
          ) : (
            <Trans>Connected. Restarting the server…</Trans>
          )}
        </p>
      </div>
    )
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        if (canSubmit) void submit()
      }}
      className="flex flex-col gap-4"
    >
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Connect your database</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">
          <Trans>Mavi CMS needs a database to store your site.</Trans>
        </p>
      </div>

      {phase === "error" && (
        <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {errorMessage}
        </p>
      )}

      <div className="grid grid-cols-2 gap-2">
        <button
          type="button"
          onClick={() => patch({ mode: "manual" })}
          className={cn(
            "rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
            form.mode === "manual"
              ? "border-primary bg-primary/5 text-primary"
              : "border-border hover:bg-muted"
          )}
        >
          <Trans>Enter details</Trans>
        </button>
        <button
          type="button"
          onClick={() => patch({ mode: "url" })}
          className={cn(
            "rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
            form.mode === "url"
              ? "border-primary bg-primary/5 text-primary"
              : "border-border hover:bg-muted"
          )}
        >
          <Trans>Connection URL</Trans>
        </button>
      </div>

      {form.mode === "url" ? (
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="db-url">
            <Trans>Connection URL</Trans>
          </Label>
          <Input
            id="db-url"
            autoFocus
            value={form.url}
            onChange={(event) => patch({ url: event.target.value })}
            placeholder="postgres://user:password@host:5432/dbname"
          />
          {preview && (
            <p className="text-xs text-muted-foreground">
              <Trans>
                {preview.engine} · {preview.host ?? "—"} ·{" "}
                {preview.database ?? "—"} · {preview.username ?? "—"}
              </Trans>
            </p>
          )}
        </div>
      ) : (
        <>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="db-engine">
              <Trans>Database type</Trans>
            </Label>
            <Select
              value={form.engine}
              onValueChange={(value) =>
                patch({
                  engine: value as DatabaseEngine,
                  port: "",
                })
              }
            >
              <SelectTrigger id="db-engine" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="postgres">PostgreSQL</SelectItem>
                <SelectItem value="mysql">MySQL</SelectItem>
                <SelectItem value="sqlite">SQLite</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {form.engine === "sqlite" ? (
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="db-path">
                <Trans>Database file path</Trans>
              </Label>
              <Input
                id="db-path"
                autoFocus
                value={form.database}
                onChange={(event) => patch({ database: event.target.value })}
                placeholder="data/mavicms.db"
              />
            </div>
          ) : (
            <>
              <div className="grid grid-cols-3 gap-2">
                <div className="col-span-2 flex flex-col gap-1.5">
                  <Label htmlFor="db-host">
                    <Trans>Host</Trans>
                  </Label>
                  <Input
                    id="db-host"
                    autoFocus
                    value={form.host}
                    onChange={(event) => patch({ host: event.target.value })}
                    placeholder="localhost"
                  />
                </div>
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="db-port">
                    <Trans>Port</Trans>
                  </Label>
                  <Input
                    id="db-port"
                    inputMode="numeric"
                    value={form.port}
                    onChange={(event) => patch({ port: event.target.value })}
                    placeholder={String(ENGINE_DEFAULT_PORT[form.engine])}
                  />
                </div>
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="db-name">
                  <Trans>Database name</Trans>
                </Label>
                <Input
                  id="db-name"
                  value={form.database}
                  onChange={(event) => patch({ database: event.target.value })}
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="db-username">
                  <Trans>Username</Trans>
                </Label>
                <Input
                  id="db-username"
                  value={form.username}
                  onChange={(event) => patch({ username: event.target.value })}
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="db-password">
                  <Trans>Password</Trans>
                </Label>
                <Input
                  id="db-password"
                  type="password"
                  value={form.password}
                  onChange={(event) => patch({ password: event.target.value })}
                />
              </div>
            </>
          )}
        </>
      )}

      <div className="flex gap-2">
        <Button type="button" variant="outline" onClick={onBack} className="flex-1">
          {t`Back`}
        </Button>
        <Button type="submit" disabled={!canSubmit} className="flex-1">
          {t`Test & continue`}
        </Button>
      </div>
    </form>
  )
}

function SiteStep({
  siteTitle,
  tagline,
  onChange,
  onBack,
  onNext,
}: {
  siteTitle: string
  tagline: string
  onChange: (fields: Partial<FormState>) => void
  onBack: () => void
  onNext: () => void
}) {
  const { t } = useLingui()
  const canContinue = siteTitle.trim().length > 0

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        if (canContinue) onNext()
      }}
      className="flex flex-col gap-4"
    >
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Tell us about your site</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">
          <Trans>You can change this later in your post settings.</Trans>
        </p>
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="setup-site-title">
          <Trans>Site title</Trans>
        </Label>
        <Input
          id="setup-site-title"
          autoFocus
          value={siteTitle}
          onChange={(event) => onChange({ siteTitle: event.target.value })}
          placeholder={t`My Mavi CMS Site`}
        />
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="setup-tagline">
          <Trans>Tagline</Trans>
        </Label>
        <Input
          id="setup-tagline"
          value={tagline}
          onChange={(event) => onChange({ tagline: event.target.value })}
          placeholder={t`Just another Mavi CMS site`}
        />
      </div>

      <div className="flex gap-2">
        <Button type="button" variant="outline" onClick={onBack} className="flex-1">
          {t`Back`}
        </Button>
        <Button type="submit" disabled={!canContinue} className="flex-1">
          {t`Continue`}
        </Button>
      </div>
    </form>
  )
}

function AccountStep({
  form,
  onChange,
  onBack,
  onSubmit,
}: {
  form: FormState
  onChange: (fields: Partial<FormState>) => void
  onBack: () => void
  onSubmit: () => void
}) {
  const { t } = useLingui()
  const [showPassword, setShowPassword] = React.useState(false)

  const strength = passwordStrength(form.adminPassword)
  const strengthLabel = {
    weak: t`Weak`,
    medium: t`Medium`,
    strong: t`Strong`,
  }[strength]
  const strengthColor = {
    weak: "bg-destructive",
    medium: "bg-amber-500",
    strong: "bg-emerald-500",
  }[strength]

  const usernameValid = /^[a-zA-Z0-9_-]{3,32}$/.test(form.adminUsername.trim())
  const emailValid = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(form.adminEmail.trim())
  const passwordValid = form.adminPassword.length >= 8
  const needsWeakConfirm = passwordValid && strength === "weak"
  const canSubmit =
    usernameValid &&
    emailValid &&
    passwordValid &&
    (!needsWeakConfirm || form.confirmWeak)

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault()
        if (canSubmit) onSubmit()
      }}
      className="flex flex-col gap-4"
    >
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Create your administrator account</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">
          <Trans>You'll use this to sign in and manage your site.</Trans>
        </p>
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="setup-username">
          <Trans>Username</Trans>
        </Label>
        <Input
          id="setup-username"
          autoFocus
          value={form.adminUsername}
          onChange={(event) => onChange({ adminUsername: event.target.value })}
          placeholder="mehmet"
        />
        {form.adminUsername.length > 0 && !usernameValid && (
          <p className="text-xs text-destructive">
            <Trans>3-32 characters: letters, numbers, _ or -</Trans>
          </p>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="setup-email">
          <Trans>Email</Trans>
        </Label>
        <Input
          id="setup-email"
          type="email"
          value={form.adminEmail}
          onChange={(event) => onChange({ adminEmail: event.target.value })}
          placeholder="you@example.com"
        />
        {form.adminEmail.length > 0 && !emailValid && (
          <p className="text-xs text-destructive">
            <Trans>Enter a valid email address</Trans>
          </p>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="setup-password">
          <Trans>Password</Trans>
        </Label>
        <div className="flex gap-1.5">
          <div className="relative flex-1">
            <Input
              id="setup-password"
              type={showPassword ? "text" : "password"}
              value={form.adminPassword}
              onChange={(event) =>
                onChange({
                  adminPassword: event.target.value,
                  confirmWeak: false,
                })
              }
              className="pr-9"
            />
            <button
              type="button"
              onClick={() => setShowPassword((value) => !value)}
              aria-label={showPassword ? t`Hide password` : t`Show password`}
              className="absolute inset-y-0 right-2 flex items-center text-muted-foreground"
            >
              {showPassword ? (
                <EyeOff className="size-4" />
              ) : (
                <Eye className="size-4" />
              )}
            </button>
          </div>
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label={t`Generate password`}
            onClick={() => {
              onChange({ adminPassword: generatePassword(), confirmWeak: false })
              setShowPassword(true)
            }}
          >
            <Sparkles />
          </Button>
        </div>

        {form.adminPassword.length > 0 && (
          <>
            <div className="flex items-center gap-2">
              <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                <span
                  className={cn(
                    "block h-full rounded-full transition-all",
                    strengthColor
                  )}
                  style={{
                    width:
                      strength === "weak"
                        ? "33%"
                        : strength === "medium"
                          ? "66%"
                          : "100%",
                  }}
                />
              </span>
              <span className="text-xs text-muted-foreground">
                {strengthLabel}
              </span>
            </div>

            {needsWeakConfirm && (
              <label className="flex items-start gap-2 text-xs text-muted-foreground">
                <Checkbox
                  checked={form.confirmWeak}
                  onCheckedChange={(checked) =>
                    onChange({ confirmWeak: checked === true })
                  }
                />
                <Trans>Confirm use of a weak password</Trans>
              </label>
            )}
          </>
        )}
      </div>

      <div className="flex gap-2">
        <Button type="button" variant="outline" onClick={onBack} className="flex-1">
          {t`Back`}
        </Button>
        <Button type="submit" disabled={!canSubmit} className="flex-1">
          {t`Install Mavi CMS`}
        </Button>
      </div>
    </form>
  )
}

function InstallingStep() {
  return (
    <div className="flex flex-col items-center gap-3 py-8 text-center">
      <Loader2 className="size-6 animate-spin text-primary" />
      <p className="text-sm font-medium">
        <Trans>Installing Mavi CMS…</Trans>
      </p>
    </div>
  )
}

function SuccessStep({
  result,
  onComplete,
}: {
  result: SetupResult
  onComplete: () => void
}) {
  return (
    <div className="flex flex-col items-center gap-4 py-4 text-center">
      <span className="flex size-12 items-center justify-center rounded-full bg-emerald-500/10 text-emerald-500">
        <Check className="size-6" />
      </span>
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Success!</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">
          <Trans>
            {result.site_title} is ready. You're signed in as{" "}
            <strong className="text-foreground">{result.admin_username}</strong>.
          </Trans>
        </p>
      </div>
      <Button onClick={onComplete} className="w-full">
        <Trans>Continue to the dashboard</Trans>
      </Button>
    </div>
  )
}

function ErrorStep({
  message,
  onRetry,
}: {
  message: string
  onRetry: () => void
}) {
  return (
    <div className="flex flex-col items-center gap-4 py-4 text-center">
      <span className="flex size-12 items-center justify-center rounded-full bg-destructive/10 text-destructive">
        <AlertTriangle className="size-6" />
      </span>
      <div>
        <h2 className="text-base font-semibold">
          <Trans>Setup failed</Trans>
        </h2>
        <p className="text-sm text-muted-foreground">{message}</p>
      </div>
      <Button onClick={onRetry} className="w-full">
        <Trans>Try again</Trans>
      </Button>
    </div>
  )
}
