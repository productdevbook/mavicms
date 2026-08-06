import type { FormField } from "@/lib/api"

/** A key other people's code has to type: letters, numbers, _ and - only. */
export function fieldName(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "_")
    .replace(/^_+|_+$/g, "")
}

export function emptyField(): FormField {
  return { name: "", label: "", type: "text", required: false, options: [] }
}
