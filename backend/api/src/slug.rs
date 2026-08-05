use uuid::Uuid;

/// Lowercases and hyphenates `input`, collapsing runs of non-alphanumeric
/// characters into a single `-` (e.g. "Web Dev!!" -> "web-dev").
///
/// Letters and digits are kept in **any** script, not just ASCII: this is a
/// multilingual CMS, and stripping non-ASCII would turn "日本語" or "Привет"
/// into an empty slug. Modern browsers and search engines handle
/// percent-encoded UTF-8 paths fine.
///
/// May still return an empty string (e.g. an emoji-only title) — use
/// [`slugify_or`] where a slug is required.
pub fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut last_dash = false;

    for c in input.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            slug.push(c);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

/// [`slugify`], falling back to `{prefix}-{random}` when the input has nothing
/// sluggable in it, so callers that require a non-empty slug always get one.
pub fn slugify_or(input: &str, prefix: &str) -> String {
    let slug = slugify(input);
    if slug.is_empty() {
        format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..8])
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_ascii_behaviour() {
        assert_eq!(slugify("Web Dev!!"), "web-dev");
        assert_eq!(slugify("  Hello   World  "), "hello-world");
    }

    #[test]
    fn preserves_non_latin_scripts() {
        // Previously every one of these collapsed to "", which made all
        // Japanese tags collide on the same empty slug.
        assert_eq!(slugify("日本語のタイトル"), "日本語のタイトル");
        assert_eq!(slugify("Привет мир"), "привет-мир");
        assert_eq!(slugify("مرحبا بالعالم"), "مرحبا-بالعالم");
    }

    #[test]
    fn preserves_accented_latin() {
        assert_eq!(slugify("Merhaba Dünya"), "merhaba-dünya");
        assert_eq!(slugify("Grüße aus Köln"), "grüße-aus-köln");
    }

    #[test]
    fn distinct_inputs_stay_distinct() {
        assert_ne!(slugify("東京"), slugify("大阪"));
    }

    #[test]
    fn falls_back_when_nothing_is_sluggable() {
        let slug = slugify_or("🎉🎉", "post");
        assert!(slug.starts_with("post-"), "got {slug}");
        assert_eq!(slug.len(), "post-".len() + 8);
    }
}
