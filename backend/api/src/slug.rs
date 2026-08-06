use uuid::Uuid;

/// Turns `input` into a URL-safe slug: entities decoded, letters transliterated
/// to ASCII, everything else collapsed into single hyphens.
///
/// `Programlar&amp;Eğitimler` becomes `programlar-egitimler`, not
/// `programlar-amp-egitimler`: content arriving from another CMS carries HTML
/// entities, and an undecoded `&amp;` leaves the word "amp" in the address.
///
/// Transliteration is deliberate. Keeping the original letters produces
/// addresses that work but percent-encode into unreadable strings and travel
/// badly through other tools; `İstanbul` becomes `istanbul` and `Привет`
/// becomes `privet`. Scripts with no ASCII rendering can still slug to nothing
/// — use [`slugify_or`] where a slug is required.
pub fn slugify(input: &str) -> String {
    slug::slugify(html_escape::decode_html_entities(input.trim()))
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
    fn decodes_entities_before_slugging() {
        // The "amp" from `&amp;` used to end up in the address.
        assert_eq!(slugify("Programlar&amp;Eğitimler"), "programlar-egitimler");
        assert_eq!(slugify("Salt &amp; Pepper"), "salt-pepper");
        assert_eq!(slugify("Caf&eacute;"), "cafe");
    }

    #[test]
    fn transliterates_turkish() {
        assert_eq!(slugify("Yazılarım"), "yazilarim");
        assert_eq!(slugify("İstanbul"), "istanbul");
        assert_eq!(
            slugify("Annem'in Sağlıklı Mutfağı"),
            "annem-in-saglikli-mutfagi"
        );
        // Case must not split what is one word.
        assert_eq!(slugify("DİYET"), slugify("Diyet"));
    }

    #[test]
    fn transliterates_other_scripts() {
        assert_eq!(slugify("Привет мир"), "privet-mir");
        assert_eq!(slugify("Merhaba Dünya"), "merhaba-dunya");
    }

    #[test]
    fn distinct_inputs_stay_distinct() {
        // These collapsed to the same empty slug before, so every one of them
        // collided with every other.
        assert_ne!(slugify("東京"), slugify("大阪"));
    }

    #[test]
    fn names_emoji_rather_than_dropping_them() {
        // Worth knowing: emoji transliterate to their names, so a title made
        // only of them still slugs to something usable.
        assert_eq!(slugify("🙂"), "slight-smile");
    }

    #[test]
    fn falls_back_when_nothing_is_sluggable() {
        assert_eq!(slugify("!!! ???"), "");
        let generated = slugify_or("!!! ???", "tag");
        assert!(generated.starts_with("tag-"));
        assert_eq!(generated.len(), "tag-".len() + 8);
        assert_eq!(slugify_or("Web Dev", "tag"), "web-dev");
    }
}
