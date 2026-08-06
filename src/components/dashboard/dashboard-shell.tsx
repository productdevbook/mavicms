import * as React from "react"
import { Link, useNavigate, useRouteContext } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  Code2,
  FolderTree,
  Globe,
  Server,
  Image,
  LayoutDashboard,
  LogOut,
  Plug,
  Rocket,
  Tags,
  UsersRound,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { logout } from "@/lib/api"
import { applySurface, surfaceLabel } from "@/lib/surface"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"

export function DashboardShell({ children }: { children: React.ReactNode }) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const { user, site } = useRouteContext({ from: "/dashboard" })

  // The server's own installation and a hosted site are the same panel; which
  // one this is decides the colour, the icon and what the tab is called.
  const kind = user.operator ? "server" : "site"
  const name = site ?? undefined
  React.useEffect(() => {
    applySurface({ kind, name })
  }, [kind, name])

  const links = [
    { to: "/dashboard", label: t`Posts`, icon: LayoutDashboard },
    { to: "/dashboard/media", label: t`Media`, icon: Image },
    { to: "/dashboard/categories", label: t`Categories`, icon: FolderTree },
    { to: "/dashboard/tags", label: t`Tags`, icon: Tags },
    { to: "/dashboard/languages", label: t`Languages`, icon: Globe },
    { to: "/dashboard/plugins", label: t`Plugins`, icon: Plug },
    { to: "/dashboard/users", label: t`People`, icon: UsersRound },
    { to: "/dashboard/api", label: t`API`, icon: Code2 },
    // Publishing is a hosted site's own pages being rebuilt. The server's own
    // installation has none — its pages are this panel.
    ...(user.operator
      ? []
      : ([{ to: "/dashboard/publish", label: t`Publish`, icon: Rocket }] as const)),
    ...(user.operator
      ? ([{ to: "/dashboard/sites", label: t`Sites`, icon: Server }] as const)
      : []),
  ] as const

  return (
    <div className="surface-bar flex min-h-svh flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <div className="flex items-center gap-2">
          <span className="surface-mark flex size-7 items-center justify-center rounded-lg text-sm font-bold text-white">
            {kind === "server" ? "S" : "M"}
          </span>
          <span className="text-sm font-semibold">{name ?? "Mavi CMS"}</span>
          <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
            {surfaceLabel(kind)}
          </span>
        </div>

        <Separator orientation="vertical" className="h-5" />

        <nav className="flex items-center gap-1">
          {links.map((link) => (
            <Link
              key={link.to}
              to={link.to}
              activeOptions={{ exact: link.to === "/dashboard" }}
              className={cn(
                "flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              )}
              activeProps={{ className: "bg-muted text-foreground" }}
            >
              <link.icon className="size-4" />
              {link.label}
            </Link>
          ))}
        </nav>

        <div className="flex-1" />

        <Button
          size="sm"
          onClick={() => navigate({ to: "/editor/new" })}
        >
          {t`New post`}
        </Button>
        <LocaleToggle />
        <ModeToggle />
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t`Sign out`}
          onClick={() => {
            void logout().finally(() => navigate({ to: "/login" }))
          }}
        >
          <LogOut />
        </Button>
      </header>

      <main className="mx-auto w-full max-w-5xl flex-1 px-6 py-8">
        {children}
      </main>
    </div>
  )
}
