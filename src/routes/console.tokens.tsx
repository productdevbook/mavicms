/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import { createFileRoute } from "@tanstack/react-router"

import { ConsoleTokens } from "@/components/console-tokens"

export const Route = createFileRoute("/console/tokens")({
  component: ConsoleTokensRoute,
})

function ConsoleTokensRoute() {
  return <ConsoleTokens />
}
