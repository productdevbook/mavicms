=== Migrate to Mavi CMS ===
Contributors: productdevbook
Tags: migration, export, cms, headless
Requires at least: 5.9
Tested up to: 6.7
Requires PHP: 7.4
Stable tag: 0.1.1
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

Sends your WordPress posts, categories, tags and images to a Mavi CMS site.

== Description ==

Mavi CMS is a self-hosted CMS with a Rust backend. This plugin moves an existing
WordPress site into it over the API: posts with their original dates and
permalinks, the category tree, tags, featured images and the images inside post
content.

Nothing in WordPress is changed or deleted. The migration is resumable — a post
that has already been sent is skipped, so you can stop and continue at any time.

Multilingual sites using Polylang or WPML keep their languages, and translations
of the same post are linked together on the Mavi CMS side.

== Installation ==

1. Upload the plugin and activate it.
2. Go to Tools → Migrate to Mavi CMS.
3. Enter the address of your Mavi CMS site and sign in.
4. Press "Start migration".

Your Mavi CMS site fetches images directly from this site, so this site has to
be reachable from it while the migration runs.

== Frequently Asked Questions ==

= Is my password stored? =

No. It is used once to sign in, and only the resulting session is kept.

= What happens if I run it twice? =

Posts that have already been migrated are skipped.

= Are images copied or hotlinked? =

Copied. Mavi CMS downloads each image and rewrites the post content to point at
its own copy. An image it cannot fetch keeps its original address, and the
failure is shown in the log.

== Changelog ==

= 0.1.1 =
* Rewrite every image URL in post content, not only the one in `src`. `srcset`
  candidates and links to the full-size file used to be left pointing at the
  old site.
* Drop `srcset` and `sizes`, since one file now replaces the whole set of
  generated sizes.

= 0.1.0 =
* First release.
