import type { FormField } from "@/lib/api"

/**
 * A key other people's code has to type: letters, numbers, _ and - only.
 *
 * Anything outside that used to become an underscore, which turned "Başlık"
 * into "ba_l_k" — the two Turkish letters that have no ASCII form of their own
 * were punched out of the middle of the word. The server has transliterated
 * properly for months, and this disagreed with it because it was written
 * separately.
 *
 * So this is now only what appears while somebody is still typing: accents are
 * decomposed and dropped, the handful of Latin letters that do not decompose
 * are named, and `GET /slug` settles it when the field loses focus. The server
 * remains the one place that decides.
 */
export function fieldName(input: string): string {
  return (
    input
      .trim()
      .toLowerCase()
      // ş → s, ü → u, é → e: the letter and its mark come apart, and the mark
      // goes. Most of the Latin alphabets are this and nothing more.
      .normalize("NFD")
      .replace(/\p{M}+/gu, "")
      // The ones with no mark to remove, which decomposition cannot help with.
      .replace(/[ı]/g, "i")
      .replace(/[ø]/g, "o")
      .replace(/[đł]/g, (letter) => (letter === "đ" ? "d" : "l"))
      .replace(/æ/g, "ae")
      .replace(/œ/g, "oe")
      .replace(/ß/g, "ss")
      .replace(/[^a-z0-9_-]+/g, "_")
      .replace(/^_+|_+$/g, "")
  )
}

export function emptyField(): FormField {
  return { name: "", label: "", type: "text", required: false, options: [] }
}
