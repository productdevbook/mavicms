/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  CheckCircle2,
  Download,
  Loader2,
  Plus,
  Trash2,
  Upload,
  XCircle,
} from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  deleteCampaign,
  deleteMailList,
  deleteMailTemplate,
  deleteSubscriber,
  getCampaigns,
  getMailLists,
  getMailLog,
  getMailTemplates,
  getSubscribers,
  importSubscribers,
  saveCampaign,
  saveMailList,
  saveMailTemplate,
  saveSubscriber,
  subscriberExportUrl,
  type Campaign,
  type MailList,
  type MailLogEntry,
  type MailTemplate,
  type Subscriber,
} from "@/lib/api"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export const Route = createFileRoute("/dashboard/mail")({
  component: MailRoute,
})

type Tab = "campaigns" | "lists" | "people" | "templates" | "log"

function MailRoute() {
  const { t } = useLingui()
  const [tab, setTab] = React.useState<Tab>("campaigns")

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Mail`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Who you write to, and what you send them. Sending goes through the Amazon SES plugin.`}
        </p>
      </div>

      <div className="mb-6 flex flex-wrap gap-1 border-b border-border">
        {(
          [
            ["campaigns", t`Campaigns`],
            ["lists", t`Lists`],
            ["people", t`People`],
            ["templates", t`Templates`],
            ["log", t`Sent`],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm font-medium",
              tab === id
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground"
            )}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === "campaigns" && <Campaigns />}
      {tab === "lists" && <Lists />}
      {tab === "people" && <People />}
      {tab === "templates" && <Templates />}
      {tab === "log" && <Log />}
    </>
  )
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <p className="rounded-xl border border-dashed border-border py-12 text-center text-sm text-muted-foreground">
      {children}
    </p>
  )
}

function Spinner() {
  return (
    <div className="flex justify-center py-16">
      <Loader2 className="size-6 animate-spin text-muted-foreground" />
    </div>
  )
}

function Campaigns() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<Campaign[] | null>(null)
  const [name, setName] = React.useState("")
  const [adding, setAdding] = React.useState(false)

  const load = React.useCallback(() => {
    getCampaigns()
      .then(setRows)
      .catch(() => toast.error(t`Could not load the campaigns`))
  }, [t])

  React.useEffect(load, [load])

  const add = async () => {
    try {
      await saveCampaign({
        name: name.trim(),
        subject: name.trim(),
        body: "",
        template_id: null,
        lists: [],
        send_at: null,
      })
      setAdding(false)
      setName("")
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not make it`
      )
    }
  }

  const label = (status: string) =>
    ({
      draft: t`Draft`,
      scheduled: t`Scheduled`,
      running: t`Going out`,
      paused: t`Paused`,
      finished: t`Sent`,
      cancelled: t`Cancelled`,
    })[status] ?? status

  if (!rows) return <Spinner />

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`New campaign`}
        </Button>
      </div>

      {rows.length === 0 ? (
        <Empty>{t`No campaigns yet`}</Empty>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div key={row.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <Link
                  to="/dashboard/mail/campaigns/$campaignId"
                  params={{ campaignId: row.id }}
                  className="truncate text-sm font-medium hover:underline"
                >
                  {row.name}
                </Link>
                <p className="truncate text-xs text-muted-foreground">
                  {label(row.status)}
                  {row.to_send > 0 &&
                    ` · ${t`${row.sent} of ${row.to_send} sent`}`}
                  {row.opened > 0 && ` · ${t`${row.opened} opened`}`}
                  {row.clicked > 0 && ` · ${t`${row.clicked} clicked`}`}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() =>
                  void deleteCampaign(row.id)
                    .then(load)
                    .catch(() => toast.error(t`Could not remove it`))
                }
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog open={adding} onOpenChange={setAdding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`New campaign`}</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <Label htmlFor="campaign-name">{t`Name`}</Label>
            <Input
              id="campaign-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void add()} disabled={!name.trim()}>
              {t`Make it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function Lists() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<MailList[] | null>(null)
  const [editing, setEditing] = React.useState<Partial<MailList> | null>(null)

  const load = React.useCallback(() => {
    getMailLists()
      .then(setRows)
      .catch(() => toast.error(t`Could not load the lists`))
  }, [t])

  React.useEffect(load, [load])

  const save = async () => {
    if (!editing) return
    try {
      await saveMailList(
        {
          name: editing.name?.trim() ?? "",
          description: editing.description ?? "",
          opt_in: editing.opt_in ?? "double",
          public: editing.public ?? true,
        },
        editing.id
      )
      setEditing(null)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    }
  }

  if (!rows) return <Spinner />

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button onClick={() => setEditing({ opt_in: "double", public: true })}>
          <Plus /> {t`New list`}
        </Button>
      </div>

      {rows.length === 0 ? (
        <Empty>{t`No lists yet`}</Empty>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div key={row.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{row.name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {t`${row.confirmed} confirmed`}
                  {row.unconfirmed > 0 && ` · ${t`${row.unconfirmed} waiting`}`}
                  {row.unsubscribed > 0 && ` · ${t`${row.unsubscribed} left`}`}
                  {" · "}
                  <span className="font-mono">{row.slug}</span>
                </p>
              </div>
              <Button variant="outline" size="sm" onClick={() => setEditing(row)}>
                {t`Edit`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() =>
                  void deleteMailList(row.id)
                    .then(load)
                    .catch(() => toast.error(t`Could not remove it`))
                }
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={editing !== null}
        onOpenChange={(open) => !open && setEditing(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing?.id ? t`Edit the list` : t`New list`}</DialogTitle>
          </DialogHeader>
          {editing && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                <Label htmlFor="list-name">{t`Name`}</Label>
                <Input
                  id="list-name"
                  value={editing.name ?? ""}
                  onChange={(event) =>
                    setEditing({ ...editing, name: event.target.value })
                  }
                  autoFocus
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="list-description">{t`What it is`}</Label>
                <Textarea
                  id="list-description"
                  rows={2}
                  value={editing.description ?? ""}
                  onChange={(event) =>
                    setEditing({ ...editing, description: event.target.value })
                  }
                />
              </div>
              <Label className="flex items-center gap-3 font-normal">
                <Switch
                  checked={(editing.opt_in ?? "double") === "double"}
                  onCheckedChange={(checked) =>
                    setEditing({
                      ...editing,
                      opt_in: checked === true ? "double" : "single",
                    })
                  }
                />
                {t`Ask people to confirm by email`}
              </Label>
              <Label className="flex items-center gap-3 font-normal">
                <Switch
                  checked={editing.public ?? true}
                  onCheckedChange={(checked) =>
                    setEditing({ ...editing, public: checked === true })
                  }
                />
                {t`People may join it from the site`}
              </Label>
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditing(null)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void save()} disabled={!editing?.name?.trim()}>
              {t`Save`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function People() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<Subscriber[] | null>(null)
  const [lists, setLists] = React.useState<MailList[]>([])
  const [search, setSearch] = React.useState("")
  const [onList, setOnList] = React.useState("")
  const [adding, setAdding] = React.useState(false)
  const [draft, setDraft] = React.useState({ email: "", name: "", lists: [] as string[] })
  const [report, setReport] = React.useState<string | null>(null)

  const load = React.useCallback(() => {
    getSubscribers({ q: search || undefined, list: onList || undefined })
      .then(setRows)
      .catch(() => toast.error(t`Could not load the people`))
  }, [search, onList, t])

  React.useEffect(load, [load])
  React.useEffect(() => {
    getMailLists().then(setLists).catch(() => setLists([]))
  }, [])

  const add = async () => {
    try {
      await saveSubscriber({
        email: draft.email.trim(),
        name: draft.name.trim(),
        lists: draft.lists,
        attributes: {},
      })
      setAdding(false)
      setDraft({ email: "", name: "", lists: [] })
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add them`
      )
    }
  }

  const upload = (file: File) => {
    if (!onList) {
      toast.error(t`Choose a list first`)
      return
    }
    void file.text().then(async (csv) => {
      try {
        const done = await importSubscribers(onList, csv)
        setReport(
          t`${done.added} added, ${done.updated} updated, ${done.skipped.length} skipped`
        )
        load()
      } catch (error) {
        toast.error(
          error instanceof ApiError ? error.message : t`Could not read the file`
        )
      }
    })
  }

  return (
    <>
      <div className="mb-4 flex flex-wrap items-end gap-2">
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t`Search an address or a name`}
          className="max-w-xs"
        />
        <select
          value={onList}
          onChange={(event) => setOnList(event.target.value)}
          className="h-9 rounded-md border border-border bg-transparent px-3 text-sm"
        >
          <option value="">{t`Every list`}</option>
          {lists.map((list) => (
            <option key={list.id} value={list.id}>
              {list.name}
            </option>
          ))}
        </select>

        <div className="flex-1" />

        <Button variant="outline" onClick={() => {
          const input = document.createElement("input")
          input.type = "file"
          input.accept = ".csv,text/csv"
          input.onchange = () => input.files?.[0] && upload(input.files[0])
          input.click()
        }}>
          <Upload /> {t`Import CSV`}
        </Button>
        <Button
          variant="outline"
          onClick={() =>
            window.open(subscriberExportUrl(onList || undefined), "_blank")
          }
        >
          <Download /> {t`Export`}
        </Button>
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`Add someone`}
        </Button>
      </div>

      {report && (
        <p className="mb-4 rounded-xl border border-border px-4 py-2 text-sm">
          {report}
        </p>
      )}

      {!rows ? (
        <Spinner />
      ) : rows.length === 0 ? (
        <Empty>{t`Nobody here yet`}</Empty>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div key={row.id} className="flex items-center gap-3 px-4 py-2.5">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm">{row.email}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {row.name || "—"}
                  {row.status === "blocked" && ` · ${t`blocked`}`}
                  {` · ${t`on ${row.lists.filter((l) => l.status === "confirmed").length} lists`}`}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() =>
                  void deleteSubscriber(row.id)
                    .then(load)
                    .catch(() => toast.error(t`Could not remove them`))
                }
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog open={adding} onOpenChange={setAdding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Add someone`}</DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="sub-email">{t`Email`}</Label>
              <Input
                id="sub-email"
                type="email"
                value={draft.email}
                onChange={(event) =>
                  setDraft({ ...draft, email: event.target.value })
                }
                autoFocus
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="sub-name">{t`Name`}</Label>
              <Input
                id="sub-name"
                value={draft.name}
                onChange={(event) =>
                  setDraft({ ...draft, name: event.target.value })
                }
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label>{t`Lists`}</Label>
              {lists.map((list) => (
                <Label key={list.id} className="flex items-center gap-2 font-normal">
                  <input
                    type="checkbox"
                    checked={draft.lists.includes(list.id)}
                    onChange={(event) =>
                      setDraft({
                        ...draft,
                        lists: event.target.checked
                          ? [...draft.lists, list.id]
                          : draft.lists.filter((id) => id !== list.id),
                      })
                    }
                  />
                  {list.name}
                </Label>
              ))}
              <p className="text-sm text-muted-foreground">
                {t`Added here counts as confirmed — say so only if they asked to be on it.`}
              </p>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void add()} disabled={!draft.email.includes("@")}>
              {t`Add`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

const STARTER = `<div style="font-family:system-ui,sans-serif;max-width:600px;margin:0 auto;padding:24px">
  {{ content }}
  <hr style="margin:32px 0;border:none;border-top:1px solid #e5e5e5">
  <p style="font-size:12px;color:#71717a">
    <a href="{{ unsubscribe_url }}">Unsubscribe</a>
  </p>
</div>`

function Templates() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<MailTemplate[] | null>(null)
  const [editing, setEditing] = React.useState<Partial<MailTemplate> | null>(null)

  const load = React.useCallback(() => {
    getMailTemplates()
      .then(setRows)
      .catch(() => toast.error(t`Could not load the templates`))
  }, [t])

  React.useEffect(load, [load])

  const save = async () => {
    if (!editing) return
    try {
      await saveMailTemplate(
        {
          name: editing.name?.trim() ?? "",
          subject: editing.subject ?? "",
          body: editing.body ?? "",
          is_default: editing.is_default ?? false,
        },
        editing.id
      )
      setEditing(null)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save it`
      )
    }
  }

  if (!rows) return <Spinner />

  return (
    <>
      <div className="mb-4 flex justify-end">
        <Button onClick={() => setEditing({ body: STARTER })}>
          <Plus /> {t`New template`}
        </Button>
      </div>

      {rows.length === 0 ? (
        <Empty>{t`No templates yet — a campaign without one sends just its own body`}</Empty>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {rows.map((row) => (
            <div key={row.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">
                  {row.name}
                  {row.is_default && (
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                      {t`default`}
                    </span>
                  )}
                </p>
              </div>
              <Button variant="outline" size="sm" onClick={() => setEditing(row)}>
                {t`Edit`}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Remove`}
                onClick={() =>
                  void deleteMailTemplate(row.id)
                    .then(load)
                    .catch(() => toast.error(t`Could not remove it`))
                }
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <Dialog
        open={editing !== null}
        onOpenChange={(open) => !open && setEditing(null)}
      >
        <DialogContent className="max-h-[90svh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>
              {editing?.id ? t`Edit the template` : t`New template`}
            </DialogTitle>
          </DialogHeader>
          {editing && (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                <Label htmlFor="tpl-name">{t`Name`}</Label>
                <Input
                  id="tpl-name"
                  value={editing.name ?? ""}
                  onChange={(event) =>
                    setEditing({ ...editing, name: event.target.value })
                  }
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="tpl-body">{t`The letterhead`}</Label>
                <Textarea
                  id="tpl-body"
                  rows={14}
                  className="font-mono text-xs"
                  value={editing.body ?? ""}
                  onChange={(event) =>
                    setEditing({ ...editing, body: event.target.value })
                  }
                />
                <p className="text-sm text-muted-foreground">
                  {t`{{ content }} is where the campaign goes. {{ name }}, {{ email }} and {{ unsubscribe_url }} are filled in for each person, along with any field you keep about them.`}
                </p>
              </div>
              <Label className="flex items-center gap-3 font-normal">
                <Switch
                  checked={editing.is_default ?? false}
                  onCheckedChange={(checked) =>
                    setEditing({ ...editing, is_default: checked === true })
                  }
                />
                {t`Use this one by default`}
              </Label>
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditing(null)}>
              {t`Cancel`}
            </Button>
            <Button onClick={() => void save()} disabled={!editing?.name?.trim()}>
              {t`Save`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

function Log() {
  const { t } = useLingui()
  const [rows, setRows] = React.useState<MailLogEntry[] | null>(null)

  React.useEffect(() => {
    getMailLog()
      .then(setRows)
      .catch(() => toast.error(t`Could not load what was sent`))
  }, [t])

  if (!rows) return <Spinner />
  if (rows.length === 0) return <Empty>{t`Nothing sent yet`}</Empty>

  return (
    <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
      {rows.map((row) => (
        <div key={row.id} className="flex items-start gap-3 px-4 py-2.5">
          {row.status === "sent" ? (
            <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-600" />
          ) : (
            <XCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm">{row.subject}</p>
            <p className="truncate text-xs text-muted-foreground">
              {row.to_address} · {new Date(row.created_at).toLocaleString()}
            </p>
            {row.detail && (
              <p className="mt-1 text-xs text-destructive">{row.detail}</p>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}
