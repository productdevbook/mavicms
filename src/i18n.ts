import { i18n } from "@lingui/core"

import { messages as trMessages } from "@/locales/tr/messages.po"
import { messages as enMessages } from "@/locales/en/messages.po"

export const locales = {
  tr: "Türkçe",
  en: "English",
} as const

export type Locale = keyof typeof locales

export const defaultLocale: Locale = "tr"

const STORAGE_KEY = "mavicms:locale"

i18n.load({ tr: trMessages, en: enMessages })

function isLocale(value: string | null): value is Locale {
  return value === "tr" || value === "en"
}

export function getStoredLocale(): Locale {
  const stored = window.localStorage.getItem(STORAGE_KEY)
  return isLocale(stored) ? stored : defaultLocale
}

export function setLocale(locale: Locale) {
  window.localStorage.setItem(STORAGE_KEY, locale)
  document.documentElement.lang = locale
  i18n.activate(locale)
}

document.documentElement.lang = getStoredLocale()
i18n.activate(getStoredLocale())

export { i18n }
