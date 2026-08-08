import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Send, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createFlowCredential,
  deleteFlowCredential,
  getFlowCredentials,
  testFlowCredential,
  type FlowCredential,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/**
 * The accounts a step sends with.
 *
 * A password typed here does not come back out: the server keeps it encrypted
 * and answers only with whether one exists. Which is why the way to find out
 * it is wrong is the button that sends something.
 */
export function FlowAccounts({ onChanged }: { onChanged?: () => void }) {
  const { t } = useLingui()

  const [held, setHeld] = React.useState<FlowCredential[] | null>(null)
  const [adding, setAdding] = React.useState(false)
  const [busy, setBusy] = React.useState(false)
  const [kind, setKind] = React.useState("smtp")
  const [name, setName] = React.useState("")
  const [host, setHost] = React.useState("")
  const [port, setPort] = React.useState("587")
  const [username, setUsername] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [from, setFrom] = React.useState("")
  const [token, setToken] = React.useState("")
  const [testing, setTesting] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    getFlowCredentials()
      .then(setHeld)
      .catch(() => setHeld([]))
  }, [])

  React.useEffect(load, [load])

  const save = async () => {
    setBusy(true)
    try {
      await createFlowCredential({
        name: name.trim(),
        kind,
        secret:
          kind === "smtp"
            ? {
                host: host.trim(),
                port: Number(port) || 587,
                username: username.trim(),
                password,
                from_address: (from || username).trim(),
                from_name: "",
              }
            : { token: token.trim() },
      })
      setAdding(false)
      setName("")
      setPassword("")
      setToken("")
      load()
      onChanged?.()
      toast.success(t`Saved`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the account`
      )
    } finally {
      setBusy(false)
    }
  }

  const tryIt = async (one: FlowCredential) => {
    const to = window.prompt(t`Send a test message to which address?`)
    if (!to) return
    setTesting(one.id)
    try {
      const said = await testFlowCredential(one.id, to)
      toast.success(said ? `${t`It went`}: ${said}` : t`It went`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`It did not go`
      )
    } finally {
      setTesting(null)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-base font-semibold">{t`Accounts`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`A mailbox to send from, or a bot to post as. Kept encrypted, and never shown again.`}
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={() => setAdding(true)}>
          <Plus /> {t`Add`}
        </Button>
      </div>

      {!held ? (
        <div className="flex justify-center py-6">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
        </div>
      ) : held.length === 0 && !adding ? (
        <p className="rounded-xl border border-dashed border-border py-8 text-center text-sm text-muted-foreground">
          {t`No accounts yet. Without one, an email step sends with the site's own mail settings.`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {held.map((one) => (
            <div key={one.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{one.name}</p>
                <p className="text-xs text-muted-foreground">{one.kind}</p>
              </div>
              {one.kind === "smtp" ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={testing === one.id}
                  onClick={() => void tryIt(one)}
                >
                  {testing === one.id ? (
                    <Loader2 className="animate-spin" />
                  ) : (
                    <Send />
                  )}
                  {t`Send a test`}
                </Button>
              ) : null}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                onClick={() => {
                  void deleteFlowCredential(one.id).then(() => {
                    load()
                    onChanged?.()
                  })
                }}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      {adding ? (
        <div className="flex flex-col gap-3 rounded-xl border border-border p-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="acc-kind">{t`What kind`}</Label>
            <Select value={kind} onValueChange={(value) => setKind(value ?? "smtp")}>
              <SelectTrigger id="acc-kind">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="smtp">{t`A mailbox (SMTP)`}</SelectItem>
                <SelectItem value="telegram">{t`A Telegram bot`}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="acc-name">{t`Name`}</Label>
            <Input
              id="acc-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t`What you will recognise it by`}
            />
          </div>

          {kind === "smtp" ? (
            <>
              <div className="grid gap-3 sm:grid-cols-[1fr_7rem]">
                <div className="flex flex-col gap-2">
                  <Label htmlFor="acc-host">{t`Server`}</Label>
                  <Input
                    id="acc-host"
                    value={host}
                    onChange={(event) => setHost(event.target.value)}
                    placeholder="smtp.example.com"
                  />
                </div>
                <div className="flex flex-col gap-2">
                  <Label htmlFor="acc-port">{t`Port`}</Label>
                  <Input
                    id="acc-port"
                    value={port}
                    onChange={(event) => setPort(event.target.value)}
                  />
                </div>
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="acc-user">{t`Username`}</Label>
                <Input
                  id="acc-user"
                  value={username}
                  onChange={(event) => setUsername(event.target.value)}
                  placeholder="biri@example.com"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="acc-pass">{t`Password`}</Label>
                <Input
                  id="acc-pass"
                  type="password"
                  autoComplete="off"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="acc-from">{t`Send as`}</Label>
                <Input
                  id="acc-from"
                  value={from}
                  onChange={(event) => setFrom(event.target.value)}
                  placeholder={t`The username, if left empty`}
                />
                <p className="text-sm text-muted-foreground">
                  {t`Most servers refuse to send as an address the account does not own. 465 is TLS from the start, 587 upgrades — the port decides unless you know otherwise.`}
                </p>
              </div>
            </>
          ) : (
            <div className="flex flex-col gap-2">
              <Label htmlFor="acc-token">{t`Bot token`}</Label>
              <Input
                id="acc-token"
                type="password"
                autoComplete="off"
                value={token}
                onChange={(event) => setToken(event.target.value)}
                placeholder="123456:ABC-DEF..."
              />
              <p className="text-sm text-muted-foreground">
                {t`From BotFather. The step also needs the chat's id, which you give it there.`}
              </p>
            </div>
          )}

          <div className="flex gap-2">
            <Button onClick={() => void save()} disabled={busy || !name.trim()}>
              {busy ? <Loader2 className="animate-spin" /> : null}
              {t`Save`}
            </Button>
            <Button variant="ghost" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  )
}
