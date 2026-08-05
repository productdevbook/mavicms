import { useLingui } from "@lingui/react/macro"

export type PostStatus = "draft" | "review" | "scheduled" | "published"

export interface PostMeta {
  title: string
  slug: string
  excerpt: string
  status: PostStatus
  publishAt: string
  author: string
  category: string
  categoryId: string | null
  tags: string[]
  coverUrl: string
  seoTitle: string
  seoDescription: string
  canonical: string
  featured: boolean
  allowComments: boolean
}

export function useStatusLabels(): Record<PostStatus, string> {
  const { t } = useLingui()
  return {
    draft: t`Draft`,
    review: t`In review`,
    scheduled: t`Scheduled`,
    published: t`Published`,
  }
}
