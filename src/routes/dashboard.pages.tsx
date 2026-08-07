/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ContentList } from "@/components/dashboard/content-list"

export const Route = createFileRoute("/dashboard/pages")({
  component: PagesRoute,
})

/** The About, the Contact — the ones that are not in the feed. */
function PagesRoute() {
  return <ContentList kind="page" />
}
