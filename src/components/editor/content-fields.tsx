import { useLingui } from "@lingui/react/macro"

import { type FormField } from "@/lib/api"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

/**
 * The fields a piece of content has because of what kind of thing it is: a
 * course's price and level, a property's rooms.
 *
 * The definitions come from the site rather than from here, so this renders
 * whatever it is handed. A required field is marked but not enforced on the
 * way in — the server refuses to publish without it, which is the moment it
 * actually matters, and a draft is half-written by definition.
 */
export function ContentFields({
  fields,
  values,
  onChange,
}: {
  fields: FormField[]
  values: Record<string, unknown>
  onChange: (values: Record<string, unknown>) => void
}) {
  const { t } = useLingui()

  if (fields.length === 0) return null

  const set = (name: string, value: unknown) =>
    onChange({ ...values, [name]: value })

  return (
    <div className="flex flex-col gap-5">
      {fields.map((field) => {
        const id = `content-field-${field.name}`
        const value = values[field.name]

        return (
          <div key={field.name} className="flex flex-col gap-1.5">
            <div className="flex items-baseline justify-between gap-2">
              <Label htmlFor={id}>{field.label || field.name}</Label>
              {field.required && (
                <span className="text-[0.7rem] text-muted-foreground">
                  {t`Needed to publish`}
                </span>
              )}
            </div>

            {field.type === "textarea" ? (
              <Textarea
                id={id}
                value={typeof value === "string" ? value : ""}
                onChange={(event) => set(field.name, event.target.value)}
              />
            ) : field.type === "checkbox" ? (
              <Switch
                id={id}
                checked={value === true}
                onCheckedChange={(next) => set(field.name, next)}
              />
            ) : field.type === "select" ? (
              <Select
                value={typeof value === "string" ? value : ""}
                onValueChange={(next) => set(field.name, next)}
              >
                <SelectTrigger id={id} className="w-full">
                  <SelectValue>
                    {(chosen: string) => chosen || t`Choose`}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {field.options.map((option) => (
                    <SelectItem key={option} value={option}>
                      {option}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input
                id={id}
                type={inputType(field.type)}
                value={
                  value === null || value === undefined ? "" : String(value)
                }
                onChange={(event) =>
                  set(
                    field.name,
                    // A number field that holds text is a value the server
                    // will refuse, so it is kept as a number here or left
                    // empty rather than sent as "12a".
                    field.type === "number"
                      ? event.target.value === ""
                        ? null
                        : Number(event.target.value)
                      : event.target.value
                  )
                }
              />
            )}
          </div>
        )
      })}
    </div>
  )
}

function inputType(kind: FormField["type"]): string {
  switch (kind) {
    case "email":
      return "email"
    case "phone":
      return "tel"
    case "number":
      return "number"
    case "date":
      return "date"
    case "url":
      return "url"
    default:
      return "text"
  }
}
