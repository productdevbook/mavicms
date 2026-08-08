import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import type {
  BackupConfig,
  BackupSettings,
  EmailSettingsPayload,
  S3SettingsPayload,
} from "@/lib/api"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/**
 * The plugin settings, from either side of them.
 *
 * A site sets its own from its panel; an agency sets its sites' from the
 * console without signing in to each. One form so the two cannot drift into
 * offering different things.
 */

function Field({
  id,
  label,
  hint,
  value,
  placeholder,
  type,
  onChange,
}: {
  id: string
  label: string
  hint?: string
  value: string
  placeholder?: string
  type?: string
  onChange: (value: string) => void
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type={type}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  )
}

export function S3Fields({
  form,
  hasStoredSecret,
  onChange,
}: {
  form: S3SettingsPayload
  hasStoredSecret: boolean
  onChange: (values: Partial<S3SettingsPayload>) => void
}) {
  const { t } = useLingui()

  return (
    <>
      <div className="flex items-start justify-between gap-3 rounded-xl border border-border p-3">
        <div className="flex flex-col">
          <Label htmlFor="s3-enabled">{t`Use S3 for new uploads`}</Label>
          <span className="text-xs text-muted-foreground">
            {t`When off, uploads are stored on the server's disk.`}
          </span>
        </div>
        <Switch
          id="s3-enabled"
          checked={form.enabled}
          onCheckedChange={(value) => onChange({ enabled: value })}
        />
      </div>

      <Field
        id="s3-endpoint"
        label={t`Endpoint`}
        hint={t`Leave empty for AWS S3`}
        value={form.endpoint}
        placeholder="https://<account>.r2.cloudflarestorage.com"
        onChange={(value) => onChange({ endpoint: value })}
      />
      <Field
        id="s3-region"
        label={t`Region`}
        value={form.region}
        placeholder="auto"
        onChange={(value) => onChange({ region: value })}
      />
      <Field
        id="s3-bucket"
        label={t`Bucket`}
        value={form.bucket}
        onChange={(value) => onChange({ bucket: value })}
      />
      <Field
        id="s3-access-key"
        label={t`Access key ID`}
        value={form.access_key_id}
        onChange={(value) => onChange({ access_key_id: value })}
      />
      <Field
        id="s3-secret"
        label={t`Secret access key`}
        type="password"
        hint={
          hasStoredSecret ? t`Stored — leave empty to keep it unchanged` : undefined
        }
        value={form.secret_access_key ?? ""}
        onChange={(value) => onChange({ secret_access_key: value })}
      />
      <Field
        id="s3-public-url"
        label={t`Public base URL`}
        hint={t`Where readers load the images from (bucket public URL or CDN)`}
        value={form.public_base_url}
        placeholder="https://cdn.example.com"
        onChange={(value) => onChange({ public_base_url: value })}
      />
      <Field
        id="s3-prefix"
        label={t`Path prefix`}
        hint={t`Optional folder inside the bucket`}
        value={form.path_prefix}
        placeholder="media"
        onChange={(value) => onChange({ path_prefix: value })}
      />
    </>
  )
}

export function BackupFields({
  settings,
  config,
  enabled,
  onEnabledChange,
  onChange,
}: {
  settings: BackupSettings
  config: BackupConfig
  enabled: boolean
  onEnabledChange: (value: boolean) => void
  onChange: (values: Partial<BackupConfig>) => void
}) {
  const { t } = useLingui()

  const SCHEDULE_LABELS: Record<string, string> = {
    off: t`Only by hand`,
    hourly: t`Every hour`,
    daily: t`Every day`,
    weekly: t`Every week`,
  }

  return (
    <>
      <div className="flex items-center justify-between rounded-xl border border-border px-4 py-3">
        <div>
          <p className="text-sm font-medium">{t`Enabled`}</p>
          <p className="text-sm text-muted-foreground">
            {t`Scheduled backups only run while this is on. You can always back up by hand below.`}
          </p>
        </div>
        <Switch checked={enabled} onCheckedChange={onEnabledChange} />
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="destination">{t`Where to keep them`}</Label>
        <Select
          value={config.destination}
          onValueChange={(value) =>
            onChange({
              destination: (value as BackupConfig["destination"]) ?? "local",
            })
          }
        >
          <SelectTrigger id="destination">
            <SelectValue>
              {(value: string) =>
                value === "s3"
                  ? settings.s3_bucket
                    ? t`S3 bucket (${settings.s3_bucket})`
                    : t`S3 bucket`
                  : t`On the server's disk`
              }
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="local">{t`On the server's disk`}</SelectItem>
            {/* Offered only once the bucket is set up, so the choice cannot
                be made before it can work. */}
            {settings.s3_available && (
              <SelectItem value="s3">
                {settings.s3_bucket
                  ? t`S3 bucket (${settings.s3_bucket})`
                  : t`S3 bucket`}
              </SelectItem>
            )}
          </SelectContent>
        </Select>
        {!settings.s3_available && (
          <p className="text-sm text-muted-foreground">
            {t`Set up the S3 plugin first if you want backups kept off this machine.`}
          </p>
        )}
      </div>

      <Field
        id="folder"
        label={t`Folder`}
        hint={
          config.destination === "s3"
            ? t`A prefix inside the bucket.`
            : t`A folder inside the data directory.`
        }
        value={config.folder}
        placeholder="backups"
        onChange={(value) => onChange({ folder: value })}
      />

      <div className="flex items-center justify-between rounded-xl border border-border px-4 py-3">
        <div>
          <p className="text-sm font-medium">{t`Include the uploaded files`}</p>
          <p className="text-sm text-muted-foreground">
            {t`Makes the archive much larger. Worth it when media sits on this machine; less so when it is already in a bucket.`}
          </p>
        </div>
        <Switch
          checked={config.include_media}
          onCheckedChange={(checked) => onChange({ include_media: checked })}
        />
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="schedule">{t`How often`}</Label>
          <Select
            value={config.schedule}
            onValueChange={(value) =>
              onChange({ schedule: (value as BackupConfig["schedule"]) ?? "off" })
            }
          >
            <SelectTrigger id="schedule">
              <SelectValue>
                {(value: string) => SCHEDULE_LABELS[value] ?? value}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {Object.entries(SCHEDULE_LABELS).map(([value, label]) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <Field
          id="keep"
          label={t`How many to keep`}
          hint={t`Older archives are removed once a new one is written.`}
          value={String(config.keep)}
          type="number"
          onChange={(value) => onChange({ keep: Math.max(1, Number(value) || 1) })}
        />
      </div>
    </>
  )
}

/**
 * Amazon SES settings, shown to whoever is setting them up — the site's own
 * people under Plugins, or the agency from the console.
 *
 * The secret key is write-only. It is never sent back, so an empty box means
 * "keep what is stored" rather than "clear it"; saying so is the difference
 * between an untouched form and one that quietly breaks sending.
 */
export function EmailFields({
  form,
  hasStoredSecret,
  onChange,
  lent = false,
}: {
  form: EmailSettingsPayload
  hasStoredSecret: boolean
  onChange: (values: Partial<EmailSettingsPayload>) => void
  /**
   * The server has lent this site its account. The region and the keys are
   * the server's, so asking for them here would be asking for the one thing
   * the arrangement exists to avoid.
   */
  lent?: boolean
}) {
  const { t } = useLingui()
  const [extra, setExtra] = React.useState({ address: "", name: "" })

  return (
    <>
      {lent ? null : (
        <>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor="ses-region">{t`Region`}</Label>
              <Input
                id="ses-region"
                value={form.region}
                onChange={(event) => onChange({ region: event.target.value })}
                placeholder="eu-central-1"
              />
              <p className="text-sm text-muted-foreground">
                {t`The one your SES identities are verified in. Verification does not cross regions.`}
              </p>
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="ses-key">{t`Access key ID`}</Label>
              <Input
                id="ses-key"
                value={form.access_key_id}
                onChange={(event) =>
                  onChange({ access_key_id: event.target.value })
                }
              />
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="ses-secret">{t`Secret access key`}</Label>
            <Input
              id="ses-secret"
              type="password"
              value={form.secret_access_key ?? ""}
              onChange={(event) =>
                onChange({ secret_access_key: event.target.value })
              }
              placeholder={
                hasStoredSecret ? t`Stored — type to replace` : t`Not set yet`
              }
            />
            <p className="text-sm text-muted-foreground">
              {t`An IAM user with ses:SendEmail. That is all sending needs. The account page below asks Amazon rather more, and says which permission is missing when one is.`}
            </p>
          </div>

        </>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="ses-from">{t`From address`}</Label>
          <Input
            id="ses-from"
            type="email"
            value={form.from_address}
            onChange={(event) =>
              onChange({ from_address: event.target.value })
            }
            placeholder="site@example.com"
          />
          <p className="text-sm text-muted-foreground">
            {t`SES refuses to send from an address or domain it has not verified. That is the usual reason a first message does not arrive.`}
          </p>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="ses-from-name">{t`From name`}</Label>
          <Input
            id="ses-from-name"
            value={form.from_name}
            onChange={(event) => onChange({ from_name: event.target.value })}
          />
        </div>
      </div>

      <div className="flex flex-col gap-2">
        <Label>{t`Other addresses you send as`}</Label>
        {form.senders.length > 0 && (
          <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
            {form.senders.map((sender, index) => (
              <div key={index} className="flex items-center gap-2 px-3 py-2">
                <span className="min-w-0 flex-1 truncate text-sm">
                  {sender.name ? `${sender.name} <${sender.address}>` : sender.address}
                </span>
                <button
                  type="button"
                  aria-label={t`Remove`}
                  className="text-xs text-muted-foreground hover:text-destructive"
                  onClick={() =>
                    onChange({
                      senders: form.senders.filter((_, at) => at !== index),
                    })
                  }
                >
                  {t`Remove`}
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="flex flex-wrap gap-2">
          <Input
            value={extra.address}
            onChange={(event) =>
              setExtra({ ...extra, address: event.target.value })
            }
            placeholder="bulten@example.com"
            className="max-w-xs"
          />
          <Input
            value={extra.name}
            onChange={(event) => setExtra({ ...extra, name: event.target.value })}
            placeholder={t`Name shown`}
            className="max-w-40"
          />
          <button
            type="button"
            disabled={!extra.address.includes("@")}
            className="rounded-md border border-border px-3 text-sm disabled:opacity-50"
            onClick={() => {
              onChange({ senders: [...form.senders, extra] })
              setExtra({ address: "", name: "" })
            }}
          >
            {t`Add`}
          </button>
        </div>
        <p className="text-sm text-muted-foreground">
          {t`A campaign can go out as any of these. Each one has to be verified in SES too — an address is not a sender until Amazon says so.`}
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Label htmlFor="ses-reply">{t`Reply-to`}</Label>
          <Input
            id="ses-reply"
            type="email"
            value={form.reply_to}
            onChange={(event) => onChange({ reply_to: event.target.value })}
            placeholder={t`Optional`}
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="ses-set">{t`Configuration set`}</Label>
          <Input
            id="ses-set"
            value={form.configuration_set}
            onChange={(event) =>
              onChange({ configuration_set: event.target.value })
            }
            placeholder={t`Optional`}
          />
        </div>
      </div>
    </>
  )
}
