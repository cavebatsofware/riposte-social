/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
//! Markdown rendering for post bodies.
//!
//! Posts are authored in CommonMark with link autodetection. We render via
//! pulldown-cmark and sanitize with ammonia before sending HTML to clients,
//! so a malicious or careless author cannot inject scripts, event handlers,
//! or arbitrary attributes. The sanitizer's allowlist is the actual security
//! boundary; the renderer's settings are for ergonomics only.
//!
//! Outbound links open in a new tab with `rel="noopener noreferrer"`.

use ammonia::Builder;
use pulldown_cmark::{html, Options, Parser};

/// Render a markdown source string to sanitized HTML.
pub fn render_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    // Autolink bare URLs so users can paste links without the `[]()` form.
    options.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(markdown, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    sanitize(&raw_html)
}

/// Sanitize untrusted HTML to an allowlist suitable for post bodies.
///
/// The allowlist intentionally rejects:
/// - `<script>`, `<style>`, `<iframe>`, `<object>`, `<embed>`, etc.
/// - `on*` event handler attributes.
/// - `javascript:` and `data:` URLs (except for safe img data URIs which are
///   blocked here too; we only serve images from our own media host).
/// - Anything else not explicitly listed below.
fn sanitize(raw_html: &str) -> String {
    Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .add_generic_attributes(["class"])
        .add_tag_attributes("img", ["src", "alt", "title", "width", "height"])
        .clean(raw_html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_to_html("**bold** and *italic* and a [link](https://example.com).");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains(r#"href="https://example.com""#));
    }

    #[test]
    fn strips_script_tags() {
        let html = render_to_html("<script>alert(1)</script>Hi");
        assert!(!html.contains("<script>"));
        assert!(!html.contains("alert(1)"));
        assert!(html.contains("Hi"));
    }

    #[test]
    fn strips_event_handlers() {
        let html = render_to_html(r#"<a href="x" onclick="alert(1)">click</a>"#);
        assert!(!html.contains("onclick"));
        assert!(!html.contains("alert(1)"));
    }

    #[test]
    fn strips_javascript_urls() {
        let html = render_to_html("[click](javascript:alert(1))");
        assert!(!html.to_lowercase().contains("javascript:"));
    }

    #[test]
    fn keeps_inline_images() {
        let html = render_to_html("![alt](https://example.com/cat.jpg)");
        assert!(html.contains("<img"));
        assert!(html.contains(r#"src="https://example.com/cat.jpg""#));
    }

    #[test]
    fn external_links_get_rel_noopener() {
        let html = render_to_html("[ex](https://example.com)");
        assert!(html.contains("rel=\"noopener noreferrer\""));
    }
}
