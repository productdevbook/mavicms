import * as React from "react"
import { useLingui } from "@lingui/react/macro"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import { ApiError, saveOrganizationBuildToken } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

/// One token for every site this agency builds, instead of the same one typed
/// into each of them.
export function ConsoleBuildToken({ stored }: { stored: boolean }) {
  const { t } = useLingui()

  const [token, setToken] = React.useState("")
  const [kept, setKept] = React.useState(stored)
  const [saving, setSaving] = React.useState(false)

  const save = async (value: string) => {
    setSaving(true)
    try {
      const account = await saveOrganizationBuildToken(value)
      setKept(account.has_build_token)
      setToken("")
      toast.success(value ? t`Saved` : t`Removed`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not save the token`
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-base font-semibold">{t`Access to your repositories`}</h2>
        <p className="text-sm text-muted-foreground">
          {t`Your sites are built from private repositories, and a build needs a token to read them with. Keep one here and every site you look after uses it — a site whose project lives somewhere else can still be given its own, and that one wins.`}
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="build-token">{t`Access token`}</Label>
        <Input
          id="build-token"
          type="password"
          autoComplete="off"
          value={token}
          onChange={(event) => setToken(event.target.value)}
          placeholder={kept ? t`Stored — type to replace` : t`Not set`}
        />
        <p className="text-sm text-muted-foreground">
          {t`Read-only access to the contents of the repositories you build, and nothing else. It is stored encrypted and never shown again.`}
        </p>
      </div>

      <div className="flex gap-2">
        <Button
          onClick={() => void save(token.trim())}
          disabled={saving || !token.trim()}
        >
          {saving ? <Loader2 className="animate-spin" /> : null}
          {t`Save`}
        </Button>
        {kept ? (
          <Button
            variant="ghost"
            onClick={() => void save("")}
            disabled={saving}
          >
            {t`Remove`}
          </Button>
        ) : null}
      </div>
    </div>
  )
}
