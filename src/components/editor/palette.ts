export interface Swatch {
  name: string
  value: string
}

type TFn = (strings: TemplateStringsArray, ...values: unknown[]) => string

export function getTextColors(t: TFn): Swatch[] {
  return [
    { name: t`Default`, value: "" },
    { name: t`Gray`, value: "#6b7280" },
    { name: t`Brown`, value: "#92400e" },
    { name: t`Orange`, value: "#ea580c" },
    { name: t`Yellow`, value: "#ca8a04" },
    { name: t`Green`, value: "#16a34a" },
    { name: t`Teal`, value: "#0891b2" },
    { name: t`Blue`, value: "#2563eb" },
    { name: t`Navy`, value: "#1e3a8a" },
    { name: t`Purple`, value: "#7c3aed" },
    { name: t`Pink`, value: "#db2777" },
    { name: t`Red`, value: "#dc2626" },
  ]
}

export function getHighlightColors(t: TFn): Swatch[] {
  return [
    { name: t`Yellow`, value: "#fef08a" },
    { name: t`Green`, value: "#bbf7d0" },
    { name: t`Blue`, value: "#bfdbfe" },
    { name: t`Purple`, value: "#e9d5ff" },
    { name: t`Pink`, value: "#fbcfe8" },
    { name: t`Orange`, value: "#fed7aa" },
    { name: t`Gray`, value: "#e5e7eb" },
    { name: t`Red`, value: "#fecaca" },
  ]
}

export function getFontFamilies(t: TFn): Swatch[] {
  return [
    { name: t`Default`, value: "" },
    { name: "Inter", value: "'Inter Variable', sans-serif" },
    { name: t`Serif`, value: "Georgia, 'Times New Roman', serif" },
    { name: t`Mono`, value: "ui-monospace, 'SF Mono', Menlo, monospace" },
    { name: t`System`, value: "system-ui, sans-serif" },
    { name: t`Cursive`, value: "'Brush Script MT', cursive" },
  ]
}

export const FONT_SIZES = [
  "12px",
  "14px",
  "16px",
  "18px",
  "20px",
  "24px",
  "30px",
  "36px",
  "48px",
]

export const LINE_HEIGHTS = ["1", "1.25", "1.5", "1.75", "2", "2.5"]
