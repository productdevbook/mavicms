/**
 * Which of the three things you are looking at.
 *
 * They are the same program on the same address family, and telling them apart
 * by reading the URL is exactly what nobody does. So each one says which it is
 * in the tab title, in the browser's icon, and in the colour along the top —
 * three places you see without looking.
 */

export type SurfaceKind = "server" | "console" | "site"

interface Surface {
  kind: SurfaceKind
  /** The agency's or the site's own name, when there is one to show. */
  name?: string
}

const LOOK: Record<SurfaceKind, { colour: string; letter: string; label: string }> = {
  // Slate: the machine itself.
  server: { colour: "#475569", letter: "S", label: "Sunucu" },
  // Violet: the agency's own surface.
  console: { colour: "#7c3aed", letter: "K", label: "Konsol" },
  // Blue: a site, which is what most people are here for most of the time.
  site: { colour: "#2563eb", letter: "M", label: "Site" },
}

/** A favicon with the right letter and colour, without a request for it. */
function icon(kind: SurfaceKind): string {
  const { colour, letter } = LOOK[kind]
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<rect width="64" height="64" rx="14" fill="${colour}"/>
<text x="32" y="45" font-family="system-ui,sans-serif" font-size="38" font-weight="700"
      fill="#fff" text-anchor="middle">${letter}</text></svg>`

  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/**
 * Names the tab, colours the icon, and marks the document so the stylesheet can
 * colour the header to match.
 */
export function applySurface({ kind, name }: Surface): void {
  const { label } = LOOK[kind]
  document.title = name ? `${name} · ${label}` : `Mavi CMS · ${label}`
  document.documentElement.dataset.surface = kind

  const link =
    document.querySelector<HTMLLinkElement>("link[rel='icon']") ??
    document.head.appendChild(Object.assign(document.createElement("link"), { rel: "icon" }))
  link.type = "image/svg+xml"
  link.href = icon(kind)
}

export function surfaceLabel(kind: SurfaceKind): string {
  return LOOK[kind].label
}
