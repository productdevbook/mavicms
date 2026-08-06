//! Markdown, rendered on the server.
//!
//! The editor sends both forms, so nothing here runs for anything a person
//! typed into the panel: it is for writing that arrives as Markdown alone —
//! an importer, or an assistant working through MCP. Before this, such a post
//! was stored with an empty body in the field every consumer reads, and looked
//! fine in the editor and blank on the site.
//!
//! The rendering is CommonMark. The panel's own is comark, which understands
//! block directives on top of it, and the two agree on everything that is not
//! one — which is all of the prose anybody writes as Markdown by hand. A
//! caller that needs the other kind sends its own HTML and this does not run.

use pulldown_cmark::{Options, Parser, html};

pub fn to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    // Deliberately not ENABLE_SMART_PUNCTUATION: it rewrites quotes and
    // dashes, and a Turkish or German post is not improved by having its
    // punctuation guessed at.

    let mut rendered = String::with_capacity(markdown.len() + markdown.len() / 4);
    html::push_html(&mut rendered, Parser::new_ext(markdown, options));
    rendered
}

#[cfg(test)]
mod tests {
    use super::to_html;

    #[test]
    fn prose_becomes_paragraphs() {
        assert_eq!(to_html("Hello\n\nAgain"), "<p>Hello</p>\n<p>Again</p>\n");
    }

    #[test]
    fn the_usual_marks_are_understood() {
        assert!(to_html("# A heading").contains("<h1>A heading</h1>"));
        assert!(to_html("- one\n- two").contains("<li>one</li>"));
        assert!(to_html("| a | b |\n|---|---|\n| 1 | 2 |").contains("<table>"));
    }

    /// Whatever a post's Markdown holds, it is written into a page. Raw HTML
    /// passing through is CommonMark's behaviour and the editor's too; what
    /// must not happen is this quietly making it *more* permissive than the
    /// field it is being stored in, which already takes HTML from the editor.
    #[test]
    fn nothing_is_added_that_was_not_written() {
        assert_eq!(to_html(""), "");
        assert_eq!(to_html("plain"), "<p>plain</p>\n");
    }
}
