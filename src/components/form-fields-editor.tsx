import { useLingui } from "@lingui/react/macro"
import { ChevronDown, ChevronUp, Plus, Trash2 } from "lucide-react"

import type { FormField, FormFieldKind } from "@/lib/api"
import { emptyField, fieldName } from "@/lib/form-fields"
import { Button } from "@/components/ui/button"
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

/**
 * The list of fields a form accepts.
 *
 * `name` is the JSON key somebody else's code will type, `label` is what a
 * person reading the answers sees. They start out the same and part company
 * the moment the label is translated, which is why both exist.
 */

const KINDS: FormFieldKind[] = [
  "text",
  "textarea",
  "email",
  "phone",
  "number",
  "checkbox",
  "select",
  "date",
  "url",
]

export function FormFieldsEditor({
  fields,
  onChange,
}: {
  fields: FormField[]
  onChange: (fields: FormField[]) => void
}) {
  const { t } = useLingui()

  const kindLabel = (kind: FormFieldKind) =>
    ({
      text: t`Text`,
      textarea: t`Long text`,
      email: t`Email`,
      phone: t`Phone`,
      number: t`Number`,
      checkbox: t`Yes or no`,
      select: t`One of a list`,
      date: t`Date`,
      url: t`Link`,
    })[kind]

  const patch = (index: number, values: Partial<FormField>) =>
    onChange(
      fields.map((field, at) => (at === index ? { ...field, ...values } : field))
    )

  const move = (index: number, by: number) => {
    const to = index + by
    if (to < 0 || to >= fields.length) return
    const next = [...fields]
    const [moved] = next.splice(index, 1)
    next.splice(to, 0, moved)
    onChange(next)
  }

  return (
    <div className="flex flex-col gap-3">
      <Label>{t`Fields`}</Label>

      {fields.map((field, index) => (
        <div
          key={index}
          className="flex flex-col gap-3 rounded-xl border border-border p-3"
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex flex-col gap-2">
              <Label htmlFor={`field-label-${index}`}>{t`Label`}</Label>
              <Input
                id={`field-label-${index}`}
                value={field.label}
                onChange={(event) => {
                  const label = event.target.value
                  // The key follows the label until somebody edits it
                  // themselves; after that it is theirs and stays put.
                  patch(index, {
                    label,
                    ...(field.name === fieldName(field.label)
                      ? { name: fieldName(label) }
                      : {}),
                  })
                }}
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor={`field-name-${index}`}>{t`Key`}</Label>
              <Input
                id={`field-name-${index}`}
                value={field.name}
                onChange={(event) =>
                  patch(index, { name: fieldName(event.target.value) })
                }
                className="font-mono"
              />
            </div>
          </div>

          <div className="flex flex-wrap items-end gap-3">
            <div className="flex min-w-40 flex-col gap-2">
              <Label htmlFor={`field-kind-${index}`}>{t`Accepts`}</Label>
              <Select
                value={field.type}
                onValueChange={(value) =>
                  patch(index, {
                    type: (value as FormFieldKind) ?? "text",
                    ...(value === "select" ? {} : { options: [] }),
                  })
                }
              >
                <SelectTrigger id={`field-kind-${index}`}>
                  <SelectValue>
                    {(value: string) => kindLabel(value as FormFieldKind)}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {KINDS.map((kind) => (
                    <SelectItem key={kind} value={kind}>
                      {kindLabel(kind)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <Label className="flex items-center gap-2 py-2 font-normal">
              <Checkbox
                checked={field.required}
                onCheckedChange={(checked) =>
                  patch(index, { required: checked === true })
                }
              />
              {t`Must be filled in`}
            </Label>

            <div className="flex-1" />

            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Move up`}
              disabled={index === 0}
              onClick={() => move(index, -1)}
            >
              <ChevronUp />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Move down`}
              disabled={index === fields.length - 1}
              onClick={() => move(index, 1)}
            >
              <ChevronDown />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t`Remove`}
              onClick={() => onChange(fields.filter((_, at) => at !== index))}
            >
              <Trash2 />
            </Button>
          </div>

          {field.type === "select" && (
            <div className="flex flex-col gap-2">
              <Label htmlFor={`field-options-${index}`}>{t`Choices`}</Label>
              <Input
                id={`field-options-${index}`}
                value={field.options.join(", ")}
                onChange={(event) =>
                  patch(index, {
                    options: event.target.value
                      .split(",")
                      .map((option) => option.trim())
                      .filter(Boolean),
                  })
                }
                placeholder={t`Separated by commas`}
              />
            </div>
          )}
        </div>
      ))}

      <div>
        <Button
          variant="outline"
          onClick={() => onChange([...fields, emptyField()])}
        >
          <Plus /> {t`Add a field`}
        </Button>
      </div>
    </div>
  )
}
