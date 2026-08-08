/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Film, Loader2, Play, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createVideo,
  deleteVideo,
  getVideo,
  getVideoPlayback,
  listVideos,
  type UploadTicket,
  type VideoAsset,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

export const Route = createFileRoute("/dashboard/videos")({
  component: VideosRoute,
})

/// How often a video still transcoding is asked about.
///
/// The host's webhook is what normally moves a row to ready. This is for the
/// site whose webhook has not been set up yet, and for the minute between a
/// upload finishing and the notice arriving.
const WHILE_WORKING = 5000

function minutes(seconds: number) {
  const whole = Math.round(seconds / 60)
  return whole < 1 ? "<1 dk" : `${whole} dk`
}

function gigabytes(bytes: number) {
  if (bytes <= 0) return "—"
  const mb = bytes / 1024 / 1024
  return mb < 1024 ? `${Math.round(mb)} MB` : `${(mb / 1024).toFixed(1)} GB`
}

/// How much goes in one request.
///
/// Small enough that a dropped connection costs one chunk rather than an
/// afternoon, large enough that a two-gigabyte lesson is not four thousand
/// round trips.
const CHUNK = 32 * 1024 * 1024

function send(
  method: string,
  url: string,
  headers: Array<[string, string]>,
  body: XMLHttpRequestBodyInit | null,
  onProgress?: (sent: number) => void
): Promise<XMLHttpRequest> {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open(method, url)
    for (const [name, value] of headers) request.setRequestHeader(name, value)
    if (onProgress) {
      request.upload.onprogress = (event) => onProgress(event.loaded)
    }
    request.onload = () =>
      request.status >= 200 && request.status < 300
        ? resolve(request)
        : reject(
            new Error(
              `${request.status} ${request.statusText}${
                request.responseText ? ` — ${request.responseText.slice(0, 200)}` : ""
              }`
            )
          )
    request.onerror = () => reject(new Error("the upload was interrupted"))
    request.send(body)
  })
}

/**
 * Sends the file to the host, not to this server, over tus.
 *
 * Written out rather than pulled in as a client because what a browser needs
 * for one file is two verbs: a POST that says how long it will be, and PATCHes
 * that say where they start. The chunking is what makes an interrupted upload
 * cost one chunk — which is the whole reason for tus, and the reason a
 * two-gigabyte file over an office connection is a feature rather than a
 * complaint.
 *
 * Nothing here is a credential. The ticket carries a signature the server made
 * for this one video, and it expires.
 */
async function upload(
  ticket: UploadTicket,
  file: File,
  onProgress: (fraction: number) => void
): Promise<void> {
  const meta = [
    `filetype ${btoa(file.type || "video/mp4")}`,
    `title ${btoa(unescape(encodeURIComponent(file.name)))}`,
  ].join(",")

  const created = await send(
    "POST",
    ticket.upload_url,
    [
      ...ticket.headers,
      ["Tus-Resumable", "1.0.0"],
      ["Upload-Length", String(file.size)],
      ["Upload-Metadata", meta],
    ],
    null
  )

  // Where the chunks go. Relative for a host that answers with a path.
  const location = created.getResponseHeader("Location")
  if (!location) throw new Error("the host accepted the upload and said nowhere to put it")
  const target = new URL(location, ticket.upload_url).toString()

  for (let offset = 0; offset < file.size; offset += CHUNK) {
    const chunk = file.slice(offset, Math.min(offset + CHUNK, file.size))
    const at = offset
    const answer = await send(
      "PATCH",
      target,
      [
        ["Tus-Resumable", "1.0.0"],
        ["Upload-Offset", String(offset)],
        ["Content-Type", "application/offset+octet-stream"],
      ],
      chunk,
      (sent) => onProgress(Math.min(1, (at + sent) / file.size))
    )

    // The host is the authority on how much it has. Trusting our own count
    // would resume from the wrong place after a chunk it only partly took.
    const acknowledged = Number(answer.getResponseHeader("Upload-Offset"))
    if (Number.isFinite(acknowledged) && acknowledged > 0) {
      offset = acknowledged - CHUNK
    }
  }

  onProgress(1)
}

function VideosRoute() {
  const { t, i18n } = useLingui()
  const navigate = useNavigate()
  const [videos, setVideos] = React.useState<VideoAsset[] | null>(null)
  const [sending, setSending] = React.useState<{
    name: string
    fraction: number
  } | null>(null)
  const [playing, setPlaying] = React.useState<{
    title: string
    url: string
  } | null>(null)
  const [pendingDelete, setPendingDelete] = React.useState<VideoAsset | null>(
    null
  )
  const chooser = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    listVideos()
      .then(setVideos)
      .catch(() => setVideos([]))
  }, [])

  React.useEffect(load, [load])

  // Anything not finished is asked about until it is.
  const working = (videos ?? []).filter(
    (video) => video.status === "uploading" || video.status === "processing"
  )
  const stillWorking = working.map((video) => video.id).join(",")

  React.useEffect(() => {
    if (!stillWorking) return
    const timer = window.setInterval(() => {
      Promise.all(stillWorking.split(",").map((id) => getVideo(id)))
        .then((fresh) =>
          setVideos((current) =>
            (current ?? []).map(
              (video) => fresh.find((one) => one.id === video.id) ?? video
            )
          )
        )
        .catch(() => {})
    }, WHILE_WORKING)
    return () => window.clearInterval(timer)
  }, [stillWorking])

  const send = async (file: File) => {
    setSending({ name: file.name, fraction: 0 })
    try {
      const { video, ticket } = await createVideo(
        file.name.replace(/\.[^.]+$/, "")
      )
      setVideos((current) => [video, ...(current ?? [])])
      await upload(ticket, file, (fraction) =>
        setSending({ name: file.name, fraction })
      )
      // The host now has the bytes; it decides when they are watchable.
      load()
    } catch (error) {
      toast.error(
        error instanceof ApiError
          ? error.message
          : error instanceof Error
            ? error.message
            : t`Could not upload it`
      )
      load()
    } finally {
      setSending(null)
    }
  }

  const play = async (video: VideoAsset) => {
    try {
      const { url } = await getVideoPlayback(video.id)
      setPlaying({ title: video.title, url })
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not play it`
      )
    }
  }

  const remove = async () => {
    if (!pendingDelete) return
    try {
      await deleteVideo(pendingDelete.id)
      load()
      toast.success(t`Video deleted`)
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not delete it`
      )
    } finally {
      setPendingDelete(null)
    }
  }

  const STATUS: Record<VideoAsset["status"], string> = {
    uploading: t`Uploading`,
    processing: t`Being prepared`,
    ready: t`Ready`,
    failed: t`Failed`,
  }

  return (
    <>
      <div className="mb-6 flex flex-col items-start gap-4 sm:flex-row sm:justify-between">
        <div>
          <h1 className="text-lg font-semibold">{t`Videos`}</h1>
          <p className="text-sm text-muted-foreground">
            {t`Lesson videos. They go straight from this browser to the host, so a two-gigabyte file never passes through the server — and every address this page makes stops working after a few hours.`}
          </p>
        </div>
        <Button
          className="shrink-0"
          disabled={sending !== null}
          onClick={() => chooser.current?.click()}
        >
          {sending ? <Loader2 className="animate-spin" /> : <Upload />}
          {t`Upload a video`}
        </Button>
        <input
          ref={chooser}
          type="file"
          accept="video/*"
          className="hidden"
          onChange={(event) => {
            const file = event.target.files?.[0]
            event.target.value = ""
            if (file) void send(file)
          }}
        />
      </div>

      {sending && (
        <div className="mb-4 rounded-xl border border-border px-4 py-3">
          <p className="truncate text-sm font-medium">{sending.name}</p>
          <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-muted">
            <div
              className="h-full rounded-full bg-primary transition-all"
              style={{ width: `${Math.round(sending.fraction * 100)}%` }}
            />
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {t`${Math.round(sending.fraction * 100)}% sent. Leaving this page stops the upload.`}
          </p>
        </div>
      )}

      {videos === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : videos.length === 0 ? (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border py-16 text-center">
          <Film className="size-5 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t`No videos yet`}</p>
          <Button
            variant="outline"
            size="sm"
            onClick={() => navigate({ to: "/dashboard/plugins/video" })}
          >
            {t`Video settings`}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {videos.map((video) => (
            <div
              key={video.id}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
            >
              {video.thumbnail_url ? (
                <img
                  src={video.thumbnail_url}
                  alt=""
                  className="h-10 w-16 shrink-0 rounded object-cover"
                />
              ) : (
                <span className="flex h-10 w-16 shrink-0 items-center justify-center rounded bg-muted">
                  <Film className="size-4 text-muted-foreground" />
                </span>
              )}

              <div className="min-w-0 basis-full sm:flex-1 sm:basis-0">
                <p className="truncate text-sm font-medium">{video.title}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {STATUS[video.status]}
                  {video.duration_seconds > 0 &&
                    ` · ${minutes(video.duration_seconds)}`}
                  {video.size_bytes > 0 && ` · ${gigabytes(video.size_bytes)}`}
                  {" · "}
                  {new Date(video.created_at).toLocaleString(i18n.locale, {
                    dateStyle: "medium",
                    timeStyle: "short",
                  })}
                  {video.error && ` · ${video.error}`}
                </p>
              </div>

              <Button
                variant="ghost"
                size="icon-sm"
                className="ml-auto"
                aria-label={t`Play`}
                disabled={video.status !== "ready"}
                onClick={() => void play(video)}
              >
                <Play />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                onClick={() => setPendingDelete(video)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      {playing && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4"
          onClick={() => setPlaying(null)}
        >
          <div
            className="w-full max-w-3xl"
            onClick={(event) => event.stopPropagation()}
          >
            <p className="mb-2 truncate text-sm text-white">{playing.title}</p>
            {/* The address is an HLS playlist, which Safari plays on its own
                and Chrome does not. A player belongs in the member area, not
                in a preview button — this is here to prove the signature. */}
            <video
              src={playing.url}
              controls
              autoPlay
              playsInline
              className="w-full rounded-lg bg-black"
            />
            <Button
              variant="outline"
              size="sm"
              className="mt-3"
              onClick={() => setPlaying(null)}
            >
              {t`Close`}
            </Button>
          </div>
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this video?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.title}" goes from the host as well as from here, and does not go to the bin. Any lesson using it will have no video.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void remove()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
