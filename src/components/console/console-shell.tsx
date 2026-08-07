import * as React from "react"
import { Link, useMatchRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { GitBranch, KeyRound, LogOut, Server, UserRound } from "lucide-react"

import { consoleLogout, type ConsoleAccount } from "@/lib/api"
import { applySurface, surfaceLabel } from "@/lib/surface"
import { Button } from "@/components/ui/button"
import { LocaleToggle } from "@/components/locale-toggle"
import { ModeToggle } from "@/components/mode-toggle"
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
 * The console an agency signs in to, in the same two halves as a site's panel.
 *
 * The sidebar is what makes the settings visible: they were all on the account
 * page, one under another, which is a fine place to put the second of them and
 * a bad place to look for the fourth.
 */
export function ConsoleShell({
  account,
  children,
}: {
  account: ConsoleAccount
  children: React.ReactNode
}) {
  const { t } = useLingui()
  const navigate = useNavigate()
  const matchRoute = useMatchRoute()

  React.useEffect(() => {
    applySurface({ kind: "console", name: account.organization_name })
  }, [account.organization_name])

  const groups = [
    {
      label: t`Sites`,
      links: [{ to: "/console", label: t`Your sites`, icon: Server }],
    },
    {
      label: t`Your agency`,
      links: [
        {
          to: "/console/repositories",
          label: t`Repositories`,
          icon: GitBranch,
        },
        { to: "/console/tokens", label: t`Access tokens`, icon: KeyRound },
        { to: "/console/account", label: t`Account`, icon: UserRound },
      ],
    },
  ]

  const signOut = async () => {
    await consoleLogout().catch(() => undefined)
    await navigate({ to: "/console/login" })
  }

  return (
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5">
            <span className="surface-mark flex size-7 shrink-0 items-center justify-center rounded-lg text-sm font-bold text-white">
              K
            </span>
            <div className="min-w-0 group-data-[collapsible=icon]:hidden">
              <p className="truncate text-sm font-semibold">
                {account.organization_name}
              </p>
              <p className="truncate text-xs text-muted-foreground">
                {surfaceLabel("console")}
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
                            // "/console" is the sites list and also the start
                            // of every other address here, so only it has to
                            // match exactly.
                            fuzzy: link.to !== "/console",
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
          <span className="text-sm font-medium">
            {account.organization_name}
          </span>

          <div className="flex-1" />

          <span className="hidden text-sm text-muted-foreground sm:inline">
            {account.email}
          </span>
          <LocaleToggle />
          <ModeToggle />
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t`Sign out`}
            onClick={() => void signOut()}
          >
            <LogOut />
          </Button>
        </header>

        <div className="mx-auto w-full max-w-4xl flex-1 px-6 py-8">
          {children}
        </div>
      </SidebarInset>
    </SidebarProvider>
  )
}
