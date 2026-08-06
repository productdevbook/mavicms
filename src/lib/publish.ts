import type { Build, BuildConfig } from "@/lib/api"

export const EMPTY_CONFIG: BuildConfig = {
  repository: "",
  branch: "main",
  build_command: "bun install --frozen-lockfile && bun run build",
  output_dir: "dist",
  has_token: false,
  environment_keys: [],
}

/** "NAME=value" lines, as the object the API takes. */
export function parseEnvironment(text: string): Record<string, string> {
  const values: Record<string, string> = {}
  for (const line of text.split("\n")) {
    const trimmed = line.trim()
    // A blank line is spacing and a "#" line is a note; neither is a variable.
    if (!trimmed || trimmed.startsWith("#")) continue
    const at = trimmed.indexOf("=")
    if (at <= 0) continue
    values[trimmed.slice(0, at).trim()] = trimmed.slice(at + 1).trim()
  }
  return values
}

export function isBusy(builds: Build[]): boolean {
  return builds.some(
    (build) => build.status === "queued" || build.status === "running"
  )
}
