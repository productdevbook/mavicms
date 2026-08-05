/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Trash2, Upload } from "lucide-react"
import { toast } from "sonner"

import { ApiError, deleteMedia, getMedia, uploadMedia, type MediaItem } from "@/lib/api"
import { formatBytes } from "@/lib/editor-utils"
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

export const Route = createFileRoute("/dashboard/media")({
  component: MediaRoute,
})

function MediaRoute() {
  const { t } = useLingui()
  const [media, setMedia] = React.useState<MediaItem[] | null>(null)
  const [pendingDelete, setPendingDelete] = React.useState<MediaItem | null>(null)
  const [uploading, setUploading] = React.useState(false)
  const fileInputRef = React.useRef<HTMLInputElement>(null)

  const load = React.useCallback(() => {
    getMedia()
      .then(setMedia)
      .catch(() => setMedia([]))
  }, [])

  React.useEffect(() => load(), [load])

  const handleUpload = async (files: FileList | null) => {
    if (!files?.length) return
    setUploading(true)
    let uploaded = 0
    for (const file of Array.from(files)) {
      try {
        await uploadMedia(file)
        uploaded += 1
      } catch (error) {
        toast.error(
          error instanceof ApiError ? error.message : t`Could not upload ${file.name}`
        )
      }
    }
    setUploading(false)
    if (fileInputRef.current) fileInputRef.current.value = ""
    if (uploaded > 0) load()
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deleteMedia(pendingDelete.id)
      setMedia((current) => current?.filter((m) => m.id !== pendingDelete.id) ?? null)
    } catch (error) {
      toast.error(error instanceof ApiError ? error.message : t`Could not delete media`)
    } finally {
      setPendingDelete(null)
    }
  }

  return (
    <>
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-lg font-semibold">{t`Media library`}</h1>
        <Button onClick={() => fileInputRef.current?.click()} disabled={uploading}>
          {uploading ? <Loader2 className="animate-spin" /> : <Upload />}
          {t`Upload`}
        </Button>
        <input
          ref={fileInputRef}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp"
          multiple
          hidden
          onChange={(event) => void handleUpload(event.target.files)}
        />
      </div>

      {media === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : media.length === 0 ? (
        <p className="py-16 text-center text-sm text-muted-foreground">
          {t`No media uploaded yet`}
        </p>
      ) : (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4">
          {media.map((item) => (
            <div
              key={item.id}
              className="group relative overflow-hidden rounded-lg border border-border"
            >
              <div className="aspect-square bg-muted/40">
                <img
                  src={item.url}
                  alt={item.filename}
                  className="size-full object-cover"
                />
              </div>
              <div className="p-2">
                <p className="truncate text-xs font-medium">{item.filename}</p>
                <p className="text-xs text-muted-foreground">
                  {formatBytes(item.size_bytes)}
                </p>
              </div>
              <Button
                variant="destructive"
                size="icon-sm"
                aria-label={t`Delete`}
                className="absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100"
                onClick={() => setPendingDelete(item)}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
        </div>
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t`Delete this media?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.filename}" will be permanently deleted. This cannot be undone.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t`Cancel`}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void confirmDelete()}>
              {t`Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
