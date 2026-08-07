import * as React from "react"
import {
  Link,
  useMatchRoute,
  useNavigate,
  useRouteContext,
} from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import {
  Code2,
  FileText,
  FolderTree,
  Globe,
  Image,
  Inbox,
  Mails,
  LayoutDashboard,
  LogOut,
  Plug,
  Rocket,
  Server,
  Tags,
  UsersRound,
  Trash2,
} from "lucide-react"

import { logout } from "@/lib/api"
import { applySurface, surfaceLabel } from "@/lib/surface"
import { Button } from "@/components/ui/button"
import { ModeToggle } from "@/components/mode-toggle"
import { LocaleToggle } from "@/components/locale-toggle"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
} from "@/components/ui/sidebar"

/**
 * The panel, in two halves.
 *
 * The header carries what is true wherever you are — which site this is, who
 * you are, writing a new post, signing out. The sidebar carries the things
 * you go *into*: the editors for posts, media, people and the rest. Keeping
 * them apart is what stops the top of the screen growing a new link every
 * time the CMS learns to do something else.
 */
export function DashboardShell({ children }: { children: React.ReactNode }) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const matchRoute = useMatchRoute()
  const { user, site } = useRouteContext({ from: "/dashboard" })

  // The server's own installation and a hosted site are the same panel; which
  // one this is decides the colour, the icon and what the tab is called.
  const kind = user.operator ? "server" : "site"
  const name = site ?? undefined
  React.useEffect(() => {
    applySurface({ kind, name })
  }, [kind, name])

  const groups = [
    {
      label: t`Content`,
      links: [
        { to: "/dashboard", label: t`Posts`, icon: LayoutDashboard },
        { to: "/dashboard/pages", label: t`Pages`, icon: FileText },
        { to: "/dashboard/media", label: t`Media`, icon: Image },
        { to: "/dashboard/categories", label: t`Categories`, icon: FolderTree },
        { to: "/dashboard/tags", label: t`Tags`, icon: Tags },
        { to: "/dashboard/forms", label: t`Forms`, icon: Inbox },
        { to: "/dashboard/mail", label: t`Mail`, icon: Mails },
        { to: "/dashboard/trash", label: t`Bin`, icon: Trash2 },
      ],
    },
    {
      label: t`This site`,
      links: [
        { to: "/dashboard/languages", label: t`Languages`, icon: Globe },
        { to: "/dashboard/plugins", label: t`Plugins`, icon: Plug },
        { to: "/dashboard/users", label: t`People`, icon: UsersRound },
        { to: "/dashboard/api", label: t`API`, icon: Code2 },
        // Publishing is a hosted site's own pages being rebuilt. The server's
        // own installation has none — its pages are this panel.
        ...(user.operator
          ? []
          : [{ to: "/dashboard/publish", label: t`Publish`, icon: Rocket }]),
        ...(user.operator
          ? [{ to: "/dashboard/sites", label: t`Sites`, icon: Server }]
          : []),
      ],
    },
  ]

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5">
            <span className="surface-mark flex size-7 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white">
              {kind === "server" ? "S" : "M"}
            </span>
            <div className="min-w-0 group-data-[collapsible=icon]:hidden">
              <p className="truncate text-sm font-semibold">
                {name ?? "Mavi CMS"}
              </p>
              <p className="truncate text-xs text-muted-foreground">
                {surfaceLabel(kind)}
              </p>
            </div>
          </div>
        </SidebarHeader>

        <SidebarContent>
          {groups.map((group) => (
            <SidebarGroup key={group.label}>
              <SidebarGroupLabel>{group.label}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {group.links.map((link) => (
                    <SidebarMenuItem key={link.to}>
                      <SidebarMenuButton
                        isActive={
                          matchRoute({
                            to: link.to,
                            fuzzy: link.to !== "/dashboard",
                          }) !== false
                        }
                        tooltip={link.label}
                        render={<Link to={link.to} />}
                      >
                        <link.icon />
                        <span>{link.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          ))}
        </SidebarContent>

        <SidebarRail />
      </Sidebar>

      <SidebarInset className="surface-bar bg-background">
        <header className="flex items-center gap-2 border-b border-border px-4 py-2">
          <SidebarTrigger />
          <span className="text-sm font-medium">{name ?? "Mavi CMS"}</span>

          <div className="flex-1" />

          <span className="hidden text-sm text-muted-foreground sm:inline">
            {user.username}
          </span>
          <Button size="sm" onClick={() => navigate({ to: "/editor/new" })}>
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

        {/* SidebarInset is the <main>; this is only what centres the page. */}
        <div className="mx-auto w-full max-w-5xl flex-1 px-6 py-8">
          {children}
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}
