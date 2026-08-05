import * as React from "react"
import { Link, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  FolderTree,
  Globe,
  Image,
  LayoutDashboard,
  LogOut,
  Plug,
  Tags,
} from "lucide-react"

import { cn } from "@/lib/utils"
import { logout } from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"

export function DashboardShell({ children }: { children: React.ReactNode }) {
  const { t } = useLingui()
  const navigate = useNavigate()

  const links = [
    { to: "/dashboard", label: t`Posts`, icon: LayoutDashboard },
    { to: "/dashboard/media", label: t`Media`, icon: Image },
    { to: "/dashboard/categories", label: t`Categories`, icon: FolderTree },
    { to: "/dashboard/tags", label: t`Tags`, icon: Tags },
    { to: "/dashboard/languages", label: t`Languages`, icon: Globe },
    { to: "/dashboard/plugins", label: t`Plugins`, icon: Plug },
  ] as const

  return (
    <div className="flex min-h-svh flex-col bg-background">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <div className="flex items-center gap-2">
          <span className="flex size-7 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground">
            M
          </span>
          <span className="text-sm font-semibold">Mavi CMS</span>
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
