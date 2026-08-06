/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { Trans, useLingui } from "@lingui/react/macro"
import { Check, Copy, ExternalLink } from "lucide-react"

import { Button } from "@/components/ui/button"

export const Route = createFileRoute("/dashboard/api")({
  component: ApiRoute,
})

/**
 * What someone building this site's pages needs to know.
 *
 * The address is the one the panel was reached on, because that is what
 * decides which site the API answers about — the same program serves every
 * site, and the hostname is the whole of the difference.
 */

interface Endpoint {
  method: string
  path: string
  what: string
}

function Snippet({ text }: { text: string }) {
  const [copied, setCopied] = React.useState(false)

  const copy = () => {
    void navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  return (
    <div className="relative">
      <pre className="overflow-x-auto rounded-lg border border-border bg-muted/40 px-3 py-2.5 pr-10 text-xs">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon-sm"
        className="absolute right-1 top-1"
        aria-label="Copy"
        onClick={copy}
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  )
}

function Table({ rows }: { rows: Endpoint[] }) {
  return (
    <div className="overflow-x-auto rounded-xl border border-border">
      <table className="w-full text-sm">
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <tr key={`${row.method} ${row.path}`}>
              <td className="whitespace-nowrap px-3 py-2 align-top font-mono text-xs text-muted-foreground">
                {row.method}
              </td>
              <td className="whitespace-nowrap px-3 py-2 align-top font-mono text-xs">
                {row.path}
              </td>
              <td className="px-3 py-2 align-top text-muted-foreground">
                {row.what}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function ApiRoute() {
  const { t } = useLingui()
  const base = `${window.location.origin}/api`

  const reading: Endpoint[] = [
    { method: "GET", path: "/posts", what: t`Posts, newest first, in pages.` },
    {
      method: "GET",
      path: "/posts?include=content",
      what: t`The same, with each body included.`,
    },
    {
      method: "GET",
      path: "/posts?locale=tr&limit=50&offset=0",
      what: t`One language, one page at a time.`,
    },
    { method: "GET", path: "/posts?q=…", what: t`Search titles, summaries and bodies.` },
    { method: "GET", path: "/posts?slug=…", what: t`One post by its address.` },
    { method: "GET", path: "/posts/{id}", what: t`One post, with its other languages.` },
    { method: "GET", path: "/categories", what: t`Categories, per language.` },
    { method: "GET", path: "/tags", what: t`Tags, per language.` },
    { method: "GET", path: "/languages", what: t`The languages this site writes in.` },
    { method: "GET", path: "/media", what: t`Uploaded files and their addresses.` },
  ]

  const writing: Endpoint[] = [
    { method: "POST", path: "/login", what: t`Sign in; sets the session cookie.` },
    { method: "POST", path: "/posts", what: t`Write a post.` },
    { method: "PUT", path: "/posts/{id}", what: t`Change one.` },
    { method: "DELETE", path: "/posts/{id}", what: t`Delete one, and any image left with no owner.` },
    { method: "POST", path: "/media", what: t`Upload a file (multipart).` },
    { method: "GET", path: "/slug?text=…", what: t`The address a title would get.` },
    { method: "POST", path: "/publish", what: t`Build this site's pages again.` },
  ]

  return (
    <>
      <div className="mb-6">
        <h1 className="text-lg font-semibold">{t`API`}</h1>
        <p className="text-sm text-muted-foreground">
          {t`Everything in this panel is this API. It is what your pages are built from, and what anything else you write can read.`}
        </p>
      </div>

      <div className="flex max-w-3xl flex-col gap-8">
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{t`Address`}</h2>
          <Snippet text={base} />
          <p className="text-sm text-muted-foreground">
            {t`Every site has its own, on its own address. The same server answers all of them and the address is what decides whose content you get — so this one returns this site's, and nobody else's.`}
          </p>
        </section>

        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{t`Signing in`}</h2>
          <p className="text-sm text-muted-foreground">
            {t`Reading needs an account on this site. Sign in once and use the cookie that comes back.`}
          </p>
          <Snippet
            text={`curl -c cookies.txt -X POST ${base}/login \\
  -H 'content-type: application/json' \\
  -d '{"username":"…","password":"…"}'

curl -b cookies.txt '${base}/posts?include=content&limit=10'`}
          />
          <p className="text-sm text-muted-foreground">
            {t`A build gets these from its build variables, which the site's agency sets — so the credentials live on the server rather than in the project.`}
          </p>
        </section>

        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-medium">{t`Reading`}</h2>
          <Table rows={reading} />
          <p className="text-sm text-muted-foreground">
            {t`A listing leaves the bodies out, because a whole archive in one response is megabytes nobody asked for. Ask for them with include=content.`}
          </p>
        </section>

        <section className="flex flex-col gap-3">
          <h2 className="text-sm font-medium">{t`Writing`}</h2>
          <Table rows={writing} />
        </section>

        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{t`Content`}</h2>
          <p className="text-sm text-muted-foreground">
            <Trans>
              A post carries <code className="font-mono text-xs">content_markdown</code>,
              which is what it is written as, and{" "}
              <code className="font-mono text-xs">content_html</code>, which is
              what it renders to. The Markdown is written for{" "}
              <a
                href="https://comark.dev"
                target="_blank"
                rel="noreferrer"
                className="underline"
              >
                comark
              </a>
              , so a project can turn its blocks into its own components.
            </Trans>
          </p>
        </section>

        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{t`The whole of it`}</h2>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => window.open("/scalar", "_blank", "noopener")}
            >
              <ExternalLink /> {t`Interactive docs`}
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                window.open("/api/api-docs/openapi.json", "_blank", "noopener")
              }
            >
              <ExternalLink /> {t`OpenAPI document`}
            </Button>
          </div>
        </section>
      </div>
    </>
  )
}
