/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"

import { ConsoleBuildToken } from "@/components/console-build-token"

export const Route = createFileRoute("/console/repositories")({
  component: ConsoleRepositoriesRoute,
})

function ConsoleRepositoriesRoute() {
  const { t } = useLingui()
  const { account } = Route.useRouteContext()

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`Repositories`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Your sites are built from private repositories, and a build needs a token to read them with. Keep one here and every site you look after uses it — a site whose project lives somewhere else can still be given its own, and that one wins.`}
        </p>
      </div>

      <ConsoleBuildToken stored={account.has_build_token} />
    </>
  )
}
