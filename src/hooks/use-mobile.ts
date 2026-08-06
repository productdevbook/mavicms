import * as React from "react"

const MOBILE_BREAKPOINT = 768

const query = () => window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`)

function subscribe(onChange: () => void) {
  const mql = query()
  mql.addEventListener("change", onChange)
  return () => mql.removeEventListener("change", onChange)
}

export function useIsMobile() {
  return React.useSyncExternalStore(
    subscribe,
    () => query().matches,
    // Rendered on a server, where there is no window and so no small screen.
    () => false
  )
}
