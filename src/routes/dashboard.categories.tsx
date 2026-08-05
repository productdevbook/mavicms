/* eslint-disable react-refresh/only-export-components -- file-based route convention */
import * as React from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useLingui } from "@lingui/react/macro"
import { Loader2, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import {
  ApiError,
  createCategory,
  deleteCategory,
  getCategories,
  type Category,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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

export const Route = createFileRoute("/dashboard/categories")({
  component: CategoriesRoute,
})

function CategoriesRoute() {
  const { t } = useLingui()
  const [categories, setCategories] = React.useState<Category[] | null>(null)
  const [name, setName] = React.useState("")
  const [pendingDelete, setPendingDelete] = React.useState<Category | null>(null)

  const load = React.useCallback(() => {
    getCategories()
      .then(setCategories)
      .catch(() => setCategories([]))
  }, [])

  React.useEffect(() => load(), [load])

  const create = async () => {
    const value = name.trim()
    if (!value) return
    try {
      const created = await createCategory(value)
      setCategories((current) => [...(current ?? []), created])
      setName("")
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not create category`
      )
    }
  }

  const confirmDelete = async () => {
    if (!pendingDelete) return
    try {
      await deleteCategory(pendingDelete.id)
      setCategories(
        (current) => current?.filter((c) => c.id !== pendingDelete.id) ?? null
      )
    } catch (error) {
      toast.error(
        error instanceof ApiError ? error.message : t`Could not delete category`
      )
    } finally {
      setPendingDelete(null)
    }
  }

  return (
    <>
      <h1 className="mb-6 text-lg font-semibold">{t`Categories`}</h1>

      <form
        onSubmit={(event) => {
          event.preventDefault()
          void create()
        }}
        className="mb-6 flex gap-2"
      >
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t`New category`}
        />
        <Button type="submit">
          <Plus /> {t`Add category`}
        </Button>
      </form>

      {categories === null ? (
        <div className="flex justify-center py-16">
          <Loader2 className="size-6 animate-spin text-muted-foreground" />
        </div>
      ) : categories.length === 0 ? (
        <p className="py-16 text-center text-sm text-muted-foreground">
          {t`No categories yet`}
        </p>
      ) : (
        <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
          {categories.map((category) => (
            <div key={category.id} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">{category.name}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {category.slug}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t`Delete`}
                onClick={() => setPendingDelete(category)}
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
            <AlertDialogTitle>{t`Delete this category?`}</AlertDialogTitle>
            <AlertDialogDescription>
              {t`"${pendingDelete?.name}" will be permanently deleted. This cannot be undone.`}
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
