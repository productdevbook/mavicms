/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { type PostKind } from "@/lib/api"
import { requireAuth } from "@/lib/auth-guard"
import { MaviEditor } from "@/components/editor/mavi-editor"

export const Route = createFileRoute("/editor/new")({
  // The language has to be settled before the first autosave creates the row —
  // picking it afterwards would leave the post in the wrong language.
  validateSearch: (
    search: Record<string, unknown>
  ): { locale?: string; translationOf?: string; kind?: PostKind } => ({
    locale: typeof search.locale === "string" ? search.locale : undefined,
    translationOf:
      typeof search.translationOf === "string" ? search.translationOf : undefined,
    kind: search.kind === "page" ? "page" : undefined,
  }),
  beforeLoad: ({ location }) => requireAuth(location.href),
  component: NewPostRoute,
})

function NewPostRoute() {
  const { locale, translationOf, kind } = Route.useSearch()
  return (
    <MaviEditor
      postId={null}
      locale={locale}
      translationOf={translationOf}
      kind={kind}
    />
  )
}
