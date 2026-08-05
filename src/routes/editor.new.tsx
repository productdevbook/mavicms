/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { requireAuth } from "@/lib/auth-guard"
import { MaviEditor } from "@/components/editor/mavi-editor"

export const Route = createFileRoute("/editor/new")({
  // The language has to be settled before the first autosave creates the row —
  // picking it afterwards would leave the post in the wrong language.
  validateSearch: (
    search: Record<string, unknown>
  ): { locale?: string; translationOf?: string } => ({
    locale: typeof search.locale === "string" ? search.locale : undefined,
    translationOf:
      typeof search.translationOf === "string" ? search.translationOf : undefined,
  }),
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: NewPostRoute,
})

function NewPostRoute() {
  const { locale, translationOf } = Route.useSearch()
  return (
    <MaviEditor postId={null} locale={locale} translationOf={translationOf} />
  )
}
