import { useLingui } from "@lingui/react/macro"

import type { FlowVocabulary } from "@/lib/api"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"

type Bag = Record<string, unknown>

function text(bag: Bag, key: string): string {
  const held = bag[key]
  return typeof held === "string" ? held : ""
}

/**
 * What a trigger offers, listed where somebody is writing the thing that uses
 * it. Nobody guesses `{{ trigger.fields.email }}` correctly from memory.
 */
function Offers({
  vocabulary,
  triggerKind,
}: {
  vocabulary: FlowVocabulary | null
  triggerKind: string
}) {
  const { t } = useLingui()
  const offers = vocabulary?.triggers.find((one) => one.kind === triggerKind)?.offers ?? []
  if (offers.length === 0) return null

  return (
    <div className="rounded-lg border border-dashed border-border p-3">
      <p className="mb-2 text-xs font-medium">{t`You can write these anywhere above`}</p>
      <div className="flex flex-col gap-1">
        {offers.map((offer) => (
          <div key={offer.path} className="flex items-baseline gap-2 text-xs">
            <code className="rounded bg-muted px-1 py-0.5">{`{{ ${offer.path} }}`}</code>
            <span className="text-muted-foreground">{offer.what}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

export function TriggerSettings({
  kind,
  config,
  onChange,
}: {
  kind: string
  config: Bag
  onChange: (config: Bag) => void
}) {
  const { t } = useLingui()
  const set = (key: string, value: unknown) => onChange({ ...config, [key]: value })

  if (kind === "form.submitted") {
    return (
      <div className="flex flex-col gap-2">
        <Label htmlFor="trigger-form">{t`Which form`}</Label>
        <Input
          id="trigger-form"
          value={text(config, "form_id")}
          onChange={(event) => set("form_id", event.target.value)}
          placeholder={t`Every form`}
        />
        <p className="text-sm text-muted-foreground">
          {t`A form's id, from the Forms screen. Left empty, every form on the site sets this off.`}
        </p>
      </div>
    )
  }

  if (kind === "post.published") {
    return (
      <div className="flex flex-col gap-2">
        <Label htmlFor="trigger-kind">{t`Which kind`}</Label>
        <Input
          id="trigger-kind"
          value={text(config, "kind")}
          onChange={(event) => set("kind", event.target.value)}
          placeholder={t`Everything`}
        />
        <p className="text-sm text-muted-foreground">
          {t`post, page, or one this site made up. Left empty, anything being published sets this off.`}
        </p>
      </div>
    )
  }

  if (kind === "schedule") {
    return (
      <div className="flex flex-col gap-2">
        <Label htmlFor="trigger-every">{t`How often, in minutes`}</Label>
        <Input
          id="trigger-every"
          type="number"
          min={1}
          value={String((config.every_minutes as number) ?? 60)}
          onChange={(event) => set("every_minutes", Number(event.target.value))}
        />
        <p className="text-sm text-muted-foreground">
          {t`Counted from when it last ran, not from a clock: 60 means about once an hour.`}
        </p>
      </div>
    )
  }

  return (
    <p className="text-sm text-muted-foreground">
      {t`Save the flow and an address appears here. Anything sent to it sets this off; the address is the only thing standing in the way, so treat it as a password.`}
    </p>
  )
}

export function StepSettings({
  action,
  config,
  onError,
  vocabulary,
  triggerKind,
  onChange,
  onErrorChange,
}: {
  action: string
  config: Bag
  onError: string
  vocabulary: FlowVocabulary | null
  triggerKind: string
  onChange: (config: Bag) => void
  onErrorChange: (value: string) => void
}) {
  const { t } = useLingui()
  const set = (key: string, value: unknown) => onChange({ ...config, [key]: value })

  return (
    <div className="flex flex-col gap-4">
      {action === "mail.send" ? (
        <>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-to">{t`To`}</Label>
            <Input
              id="step-to"
              value={text(config, "to")}
              onChange={(event) => set("to", event.target.value)}
              placeholder="biri@example.com"
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-subject">{t`Subject`}</Label>
            <Input
              id="step-subject"
              value={text(config, "subject")}
              onChange={(event) => set("subject", event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-body">{t`Message`}</Label>
            <Textarea
              id="step-body"
              rows={6}
              value={text(config, "body")}
              onChange={(event) => set("body", event.target.value)}
            />
          </div>
          <p className="text-sm text-muted-foreground">
            {t`Sent with this site's own mail settings, from Plugins → Email.`}
          </p>
        </>
      ) : null}

      {action === "http.request" ? (
        <>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-url">{t`Address`}</Label>
            <Input
              id="step-url"
              value={text(config, "url")}
              onChange={(event) => set("url", event.target.value)}
              placeholder="https://example.com/hook"
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-method">{t`Method`}</Label>
            <Select
              value={text(config, "method") || "POST"}
              onValueChange={(value) => set("method", value ?? "POST")}
            >
              <SelectTrigger id="step-method">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {["POST", "GET", "PUT", "PATCH", "DELETE"].map((one) => (
                  <SelectItem key={one} value={one}>
                    {one}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-payload">{t`What to send`}</Label>
            <Textarea
              id="step-payload"
              rows={5}
              value={text(config, "body")}
              onChange={(event) => set("body", event.target.value)}
              placeholder='{"message": "{{ trigger.fields.mesaj }}"}'
            />
          </div>
          <p className="text-sm text-muted-foreground">
            {t`Only addresses on the public internet. An address inside this cluster is refused, which is what keeps a flow from being a way to read the server's own services.`}
          </p>
        </>
      ) : null}

      {action === "branch" ? (
        <>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-left">{t`This`}</Label>
            <Input
              id="step-left"
              value={text(config, "left")}
              onChange={(event) => set("left", event.target.value)}
              placeholder="{{ trigger.fields.email }}"
            />
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="step-test">{t`Compared how`}</Label>
            <Select
              value={text(config, "test") || "equals"}
              onValueChange={(value) => set("test", value ?? "equals")}
            >
              <SelectTrigger id="step-test">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="equals">{t`is exactly`}</SelectItem>
                <SelectItem value="contains">{t`contains`}</SelectItem>
                <SelectItem value="not_empty">{t`was filled in`}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          {text(config, "test") !== "not_empty" ? (
            <div className="flex flex-col gap-2">
              <Label htmlFor="step-right">{t`That`}</Label>
              <Input
                id="step-right"
                value={text(config, "right")}
                onChange={(event) => set("right", event.target.value)}
              />
            </div>
          ) : null}
          <p className="text-sm text-muted-foreground">
            {t`If it does not match, the steps after this one are skipped and the run is still counted as a success.`}
          </p>
        </>
      ) : null}

      <div className="flex flex-col gap-2">
        <Label htmlFor="step-onerror">{t`If this step fails`}</Label>
        <Select value={onError} onValueChange={(value) => onErrorChange(value ?? "stop")}>
          <SelectTrigger id="step-onerror">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="stop">{t`Stop the whole flow`}</SelectItem>
            <SelectItem value="continue">{t`Carry on anyway`}</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Offers vocabulary={vocabulary} triggerKind={triggerKind} />
    </div>
  )
}
