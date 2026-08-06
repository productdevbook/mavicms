/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { KeyRound, Loader2, Plus, Trash2, UserRound } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  changeOwnPassword,
  createUser,
  deleteUser,
  getUsers,
  updateUser,
  type SiteUser,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

export const Route = createFileRoute("/dashboard/users")({
  component: UsersRoute,
})

const MINIMUM_PASSWORD = 10

function UsersRoute() {
  const { t } = useLingui()
  const { user: me } = Route.useRouteContext() as {
    user: { username: string }
  }

  const [people, setPeople] = React.useState<SiteUser[] | null>(null)
  const [adding, setAdding] = React.useState(false)
  const [username, setUsername] = React.useState("")
  const [email, setEmail] = React.useState("")
  const [password, setPassword] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [resetting, setResetting] = React.useState<SiteUser | null>(null)
  const [newPassword, setNewPassword] = React.useState("")
  const [removing, setRemoving] = React.useState<SiteUser | null>(null)

  // Changing your own is a different act from setting somebody else's, and
  // asks for the current one.
  const [ownCurrent, setOwnCurrent] = React.useState("")
  const [ownNext, setOwnNext] = React.useState("")

  const load = React.useCallback(() => {
    getUsers()
      .then(setPeople)
      .catch(() => toast.error(t`Could not load the accounts`))
  }, [t])

  React.useEffect(load, [load])

  const add = async () => {
    setSaving(true)
    try {
      await createUser({
        username: username.trim(),
        email: email.trim(),
        password,
      })
      setAdding(false)
      setUsername("")
      setEmail("")
      setPassword("")
      load()
      toast.success(t`Account added`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not add the account`
      )
    } finally {
      setSaving(false)
    }
  }

  const reset = async () => {
    if (!resetting) return
    setSaving(true)
    try {
      await updateUser(resetting.id, { password: newPassword })
      setResetting(null)
      setNewPassword("")
      load()
      toast.success(t`Password changed`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not change it`
      )
    } finally {
      setSaving(false)
    }
  }

  const remove = async () => {
    if (!removing) return
    try {
      await deleteUser(removing.id)
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not remove it`
      )
    } finally {
      setRemoving(null)
    }
  }

  const changeMine = async () => {
    setSaving(true)
    try {
      await changeOwnPassword(ownCurrent, ownNext)
      setOwnCurrent("")
      setOwnNext("")
      // Every session went, including this one.
      toast.success(t`Password changed — sign in again`)
      window.location.href = "/admin/login"
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not change it`
      )
      setSaving(false)
    }
  }

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold">{t`People`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Who can sign in to this site and write on it.`}
          </p>
        </div>
        <Button onClick={() => setAdding(true)}>
          <Plus /> {t`Add someone`}
        </Button>
      </div>

      <div className="flex max-w-2xl flex-col gap-8">
        {!people ? (
          <div className="flex justify-center py-16">
            <Loader2 className="size-6 animate-spin text-muted-foreground" />
          </div>
        ) : (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {people.map((person) => (
              <div key={person.id} className="flex items-center gap-3 px-4 py-3">
                <UserRound className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    {person.username}
                    {person.username === me.username && (
                      <span className="ml-2 text-xs text-muted-foreground">
                        {t`you`}
                      </span>
                    )}
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {person.email}
                    {!person.can_sign_in && ` · ${t`signs in from the console`}`}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setResetting(person)}
                >
                  <KeyRound /> {t`Set a password`}
                </Button>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t`Remove`}
                  disabled={person.username === me.username}
                  onClick={() => setRemoving(person)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
          </div>
        )}

        <div className="flex flex-col gap-3">
          <h2 className="text-sm font-medium">{t`Your own password`}</h2>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="own-current">{t`Current password`}</Label>
              <Input
                id="own-current"
                type="password"
                value={ownCurrent}
                onChange={(event) => setOwnCurrent(event.target.value)}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="own-next">{t`New password`}</Label>
              <Input
                id="own-next"
                type="password"
                value={ownNext}
                onChange={(event) => setOwnNext(event.target.value)}
              />
            </div>
          </div>
          <div>
            <Button
              onClick={() => void changeMine()}
              disabled={
                saving || !ownCurrent || ownNext.length < MINIMUM_PASSWORD
              }
            >
              {t`Change it`}
            </Button>
          </div>
          <p className="text-sm text-muted-foreground">
            {t`At least ${MINIMUM_PASSWORD} characters. Changing it signs you out everywhere, this browser included.`}
          </p>
        </div>
      </div>

      <Dialog open={adding} onOpenChange={setAdding}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Add someone`}</DialogTitle>
            <DialogDescription>
              {t`They can write, publish and manage this site.`}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              <Label htmlFor="new-username">{t`Username`}</Label>
              <Input
                id="new-username"
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                autoFocus
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="new-email">{t`Email`}</Label>
              <Input
                id="new-email"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="new-password">{t`Password`}</Label>
              <Input
                id="new-password"
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAdding(false)}>
              {t`Cancel`}
            </Button>
            <Button
              onClick={() => void add()}
              disabled={
                saving ||
                !username.trim() ||
                !email.includes("@") ||
                password.length < MINIMUM_PASSWORD
              }
            >
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Add`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={resetting !== null}
        onOpenChange={(open) => !open && setResetting(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t`Set a password`}</DialogTitle>
            <DialogDescription>
              {t`${resetting?.username} will be signed out everywhere and can sign in again with this.`}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-2">
            <Label htmlFor="reset-password">{t`New password`}</Label>
            <Input
              id="reset-password"
              type="password"
              value={newPassword}
              onChange={(event) => setNewPassword(event.target.value)}
              autoFocus
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setResetting(null)}>
              {t`Cancel`}
            </Button>
            <Button
              onClick={() => void reset()}
              disabled={saving || newPassword.length < MINIMUM_PASSWORD}
            >
              {saving ? <Loader2 className="animate-spin" /> : null}
              {t`Set it`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Remove this account?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`${removing?.username} will no longer be able to sign in. Their posts stay.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Remove`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
