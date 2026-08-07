import * as React from "react"
import { useLingui } from "@lingui/react/macro"

import { listContentTypes, type ContentType } from "@/lib/api"

/**
 * What this site publishes. Loaded once per mount, like the languages: the
 * list is small, changes rarely, and refetching costs less than a cache that
 * has to be told when it is wrong.
 */
export function useContentTypes() {
  const { t } = useLingui()
  const [loaded, setLoaded] = React.useState<ContentType[]>([])
  const [loading, setLoading] = React.useState(true)
  // Bumped rather than calling the fetch again, so that reloading is a change
  // of state the effect reacts to rather than a second thing that sets it.
  const [asOf, setAsOf] = React.useState(0)

  React.useEffect(() => {
    let cancelled = false

    listContentTypes()
      .then((all) => {
        if (!cancelled) setLoaded(all)
      })
      .catch(() => {
        if (!cancelled) setLoaded([])
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [asOf])

  const types = React.useMemo(() => {
    // The two kinds every site starts with are written into the database in
    // English by the migration that creates them, which is why a Turkish panel
    // said "Henüz Posts yok". They are named after things the panel has its own
    // words for — but only while they still carry the seeded name: rename one
    // and what you typed is what you get, here as everywhere else.
    const seeded: Record<string, { name: string; plural: string }> = {
      post: { name: t`Post`, plural: t`Posts` },
      page: { name: t`Page`, plural: t`Pages` },
    }
    return loaded.map((kind) => {
      const ours = kind.built_in ? seeded[kind.slug] : undefined
      const untouched =
        kind.slug === "post"
          ? kind.name === "Post" && kind.plural === "Posts"
          : kind.name === "Page" && kind.plural === "Pages"
      return ours && untouched ? { ...kind, ...ours } : kind
    })
  }, [loaded, t])

  const find = React.useCallback(
    (slug: string) => types.find((kind) => kind.slug === slug),
    [types]
  )

  const reload = React.useCallback(() => setAsOf((count) => count + 1), [])

  return { types, loading, find, reload }
}
