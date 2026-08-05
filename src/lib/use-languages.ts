import * as React from "react"

import { getLanguages, type Language } from "@/lib/api"

/**
 * The site's content languages. Loaded once per mount — the list is tiny and
 * changes rarely, so refetching per screen is cheaper than adding a cache.
 */
export function useLanguages() {
  const [languages, setLanguages] = React.useState<Language[]>([])
  const [loading, setLoading] = React.useState(true)

  React.useEffect(() => {
    getLanguages()
      .then((all) => setLanguages(all.filter((language) => language.is_active)))
      .catch(() => setLanguages([]))
      .finally(() => setLoading(false))
  }, [])

  const defaultCode =
    languages.find((language) => language.is_default)?.code ??
    languages[0]?.code ??
    ""

  const label = React.useCallback(
    (code: string) =>
      languages.find((language) => language.code === code)?.native_name ?? code,
    [languages]
  )

  return { languages, loading, defaultCode, label }
}
