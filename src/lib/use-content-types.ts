import * as React from "react"

import { listContentTypes, type ContentType } from "@/lib/api"

/**
 * What this site publishes. Loaded once per mount, like the languages: the
 * list is small, changes rarely, and refetching costs less than a cache that
 * has to be told when it is wrong.
 */
export function useContentTypes() {
  const [types, setTypes] = React.useState<ContentType[]>([])
  const [loading, setLoading] = React.useState(true)
  // Bumped rather than calling the fetch again, so that reloading is a change
  // of state the effect reacts to rather than a second thing that sets it.
  const [asOf, setAsOf] = React.useState(0)

  React.useEffect(() => {
    let cancelled = false

    listContentTypes()
      .then((all) => {
        if (!cancelled) setTypes(all)
      })
      .catch(() => {
        if (!cancelled) setTypes([])
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [asOf])

  const find = React.useCallback(
    (slug: string) => types.find((kind) => kind.slug === slug),
    [types]
  )

  const reload = React.useCallback(() => setAsOf((count) => count + 1), [])

  return { types, loading, find, reload }
}
