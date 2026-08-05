import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { RouterProvider, createRouter } from "@tanstack/react-router"
import { I18nProvider } from "@lingui/react"

import "./index.css"
import { routeTree } from "./routeTree.gen"
import { i18n } from "@/i18n"
import { ThemeProvider } from "@/components/theme-provider.tsx"

const router = createRouter({ routeTree })

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider i18n={i18n}>
      <ThemeProvider>
        <RouterProvider router={router} />
      </ThemeProvider>
    </I18nProvider>
  </StrictMode>
)
