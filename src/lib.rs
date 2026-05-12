use data_encoding::BASE64;
use std::{borrow::Cow, fmt::Write as _};

use html5ever::{Attribute, ParseOpts, parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

#[derive(Debug, Default)]
struct Context {
    tag_stack: Vec<Option<Box<str>>>,
    output: String,
}

#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn parse_html(html: &str) -> String {
    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        // SAFETY: we are reading from a string
        .unwrap();

    let mut ctx = Context::default();

    walk(&dom.document, &mut ctx);

    cleanup(&ctx.output)
}

fn cleanup(output: &str) -> String {
    output.trim().to_owned()
}

fn current_list_depth(ctx: &Context) -> usize {
    ctx.tag_stack
        .iter()
        .filter(|tag| matches!(tag.as_deref(), Some("ol" | "ul" | "menu")))
        .count()
}

fn inside_list_item(ctx: &Context) -> bool {
    ctx.tag_stack
        .iter()
        .rev()
        .any(|tag| matches!(tag.as_deref(), Some("li")))
}

fn list_item_continuation_width(ctx: &Context) -> Option<usize> {
    inside_list_item(ctx).then(|| current_list_depth(ctx) * 2)
}

fn write_list_item_continuation_indent(ctx: &mut Context) {
    if let Some(width) = list_item_continuation_width(ctx) {
        ctx.output
            .write_fmt(format_args!("{: <width$}", "", width = width))
            // SAFETY: we are writing to a String
            .unwrap();
    }
}

#[allow(clippy::too_many_lines)]
fn walk(node: &Handle, ctx: &mut Context) {
    match &node.data {
        NodeData::Document
        | NodeData::Doctype { .. }
        | NodeData::ProcessingInstruction { .. }
        | NodeData::Comment { .. } => walk_descendants(node, ctx, None),
        NodeData::Text { contents } => {
            // Consider:
            // - inside <pre> or <code> tags
            // - trimmed len == 0
            // - last char is a space or a newline
            // - escaping text
            // - remove excess whitespace, newlines, and carriage returns
            let text = contents.borrow();

            let escaped_text = escape_html(text.trim());
            ctx.output.push_str(&escaped_text);
        }
        NodeData::Element { name, attrs, .. } => {
            // Consider:
            // - inside <pre>
            let tag_name = name.local.as_ref();

            match tag_name {
                "hr" | "q" | "cite" | "details" | "summary" | "pre" | "code" | "table"
                | "iframe" => {
                    todo!("{tag_name}")
                }
                "sub" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push_str("#sub[");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push(']');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "sup" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push_str("#super[");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push(']');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "div" | "section" | "header" | "footer" => {
                    ctx.output.push_str("\n\n");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push_str("\n\n");
                }
                "li" => {
                    let mut tag_iter = ctx.tag_stack.iter().rev().filter_map(|t| {
                        let t = t.as_deref();
                        if matches!(t, Some("ol" | "ul" | "menu")) {
                            t
                        } else {
                            None
                        }
                    });
                    let parent_tag = tag_iter.next();
                    let tag_level =
                        list_item_continuation_width(ctx).map_or(0, |width| width.saturating_sub(2));
                    match parent_tag {
                        Some("ol") => {
                            ctx.output
                                .write_fmt(format_args!("{: <width$}+ ", "", width = tag_level))
                                // SAFETY: we are writing to a String
                                .unwrap();
                        }
                        Some("ul" | "menu") | None => {
                            ctx.output
                                .write_fmt(format_args!("{: <width$}- ", "", width = tag_level))
                                // SAFETY: we are writing to a String
                                .unwrap();
                        }
                        _ => unreachable!(),
                    }
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    if !ctx.output.ends_with("\n\n") {
                        ctx.output.push('\n');
                    }
                }
                "ol" | "ul" | "menu" => {
                    ctx.output.push('\n');
                    if ctx
                        .tag_stack
                        .iter()
                        .rev()
                        .filter_map(|t| {
                            let t = t.as_deref();
                            if matches!(t, Some("ol" | "ul" | "menu")) {
                                t
                            } else {
                                None
                            }
                        })
                        .count()
                        == 0
                    {
                        ctx.output.push('\n');
                    }
                    // TODO: extra newline if not inside a list
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    if inside_list_item(ctx) {
                        ctx.output.push('\n');
                    } else {
                        ctx.output.push_str("\n\n");
                    }
                }
                "s" | "del" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push_str("#strike[");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push(']');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "b" | "strong" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push('*');
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push('*');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "i" | "em" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push('_');
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push('_');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "u" | "ins" => {
                    let (leading_ws, trailing_ws) = inline_edge_whitespace(node);
                    if leading_ws {
                        ctx.output.push(' ');
                    }
                    ctx.output.push_str("#underline[");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push(']');
                    if trailing_ws {
                        ctx.output.push(' ');
                    }
                }
                "blockquote" => {
                    ctx.output.push_str("\n\n#quote(block: true)[\n");
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push_str("\n]\n\n");
                }
                level @ ("h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
                    let level = usize::from(level.as_bytes()[1] - b'0');
                    ctx.output
                        .write_fmt(format_args!("{:=<width$} ", "", width = level))
                        // SAFETY: we are writing to a String
                        .unwrap();
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    if let Some(id) = get_attr_value(&attrs.borrow(), "id") {
                        ctx.output
                            // TODO: escape?
                            .write_fmt(format_args!(" <{id}>\n"))
                            // SAFETY: we are writing to a String
                            .unwrap();
                    }
                }
                "html" | "head" | "body" => walk_descendants(node, ctx, Some(Box::from(tag_name))),
                "p" => {
                    if list_item_continuation_width(ctx).is_some()
                        && ctx.output.ends_with("\n\n")
                    {
                        write_list_item_continuation_indent(ctx);
                    }
                    walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    ctx.output.push_str("\n\n");
                }
                "br" => {
                    ctx.output.push_str("\\\n");
                    write_list_item_continuation_indent(ctx);
                }
                "a" => {
                    if let Some(href) = get_attr_value(&attrs.borrow(), "href") {
                        ctx.output
                            // TODO: escape href ?
                            .write_fmt(format_args!(r#"#link("{href}")["#))
                            // SAFETY: we are writing to a string
                            .unwrap();
                        walk_descendants(node, ctx, Some(Box::from(tag_name)));
                        ctx.output.push(']');
                    } else {
                        walk_descendants(node, ctx, Some(Box::from(tag_name)));
                    }
                }
                "img" => {
                    let attrs = attrs.borrow();

                    // TODO: check if the escaping is correct
                    let src = get_attr_value(&attrs, "src").map(|x| {
                        let cleared = x.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                        if let Some(stripped) = cleared.strip_prefix("data:") {
                            let uri_scheme: Vec<&str> = stripped.split(';').collect();
                            assert!(
                                uri_scheme[0].starts_with("image/"),
                                "Image tag `src` in URI scheme isn't image data."
                            );

                            let data_part: Vec<&str> =
                                uri_scheme.last().unwrap().split(',').collect();
                            match data_part[0] {
                                "base64" => {
                                    let data = BASE64
                                        .decode(data_part[1].as_bytes())
                                        .expect("Image tag `src` in URI scheme doesn't contain valid base64");

                                    return format!(
                                        "bytes(({}))",
                                        data.iter()
                                            .map(std::string::ToString::to_string)
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    );
                                }
                                "image/svg+xml" => todo!(),
                                _ => panic!(
                                    "Image tag `src` URI scheme encoding of `{}` isn't supported.",
                                    data_part[0]
                                ),
                            }
                        }
                        format!("\"{}\"", escape_quotes(x))
                    });
                    let alt = get_attr_value(&attrs, "alt");

                    match (src, alt) {
                        (Some(src), Some(alt)) => {
                            ctx.output
                                .write_fmt(format_args!(
                                    r#"#figure(caption: [{alt}], image(alt: "{}", {}))"#,
                                    escape_quotes(alt),
                                    &src
                                ))
                                // SAFETY: we are writing to a string
                                .unwrap();
                        }
                        (Some(src), None) => {
                            // TODO: test the escaping
                            ctx.output
                                .write_fmt(format_args!(r"#figure(caption: none, image({}))", &src))
                                // SAFETY: we are writing to a string
                                .unwrap();
                        }
                        _ => {}
                    }
                }
                _ => {
                    todo!()
                }
            }
        }
    }
}

fn get_attr_value<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attr| attr.name.local.as_ref() == name)
        .map(|attr| attr.value.as_ref())
}

fn text_content(node: &Handle, output: &mut String) {
    match &node.data {
        NodeData::Text { contents } => output.push_str(&contents.borrow()),
        _ => {
            for child in node.children.borrow().iter() {
                text_content(child, output);
            }
        }
    }
}

fn inline_edge_whitespace(node: &Handle) -> (bool, bool) {
    let mut text = String::new();
    text_content(node, &mut text);

    (
        text.starts_with(char::is_whitespace),
        text.ends_with(char::is_whitespace),
    )
}

fn walk_descendants(node: &Handle, ctx: &mut Context, tag_name: Option<Box<str>>) {
    ctx.tag_stack.push(tag_name);

    for child in node.children.borrow().iter() {
        walk(child, ctx);
    }

    ctx.tag_stack.pop();
}

fn escape_quotes(html: &str) -> Cow<'_, str> {
    if !html.contains('"') {
        return Cow::Borrowed(html);
    }

    let mut escaped = vec![];

    let bytes = html.as_bytes();

    for &ch in bytes {
        if matches!(ch, b'"') {
            escaped.push(b'\\');
        }
        escaped.push(ch);
    }

    Cow::Owned(
        String::from_utf8(escaped)
            // SAFETY: we started with valid utf8
            .unwrap(),
    )
}

fn escape_html(html: &str) -> Cow<'_, str> {
    if !html.contains(['*', '_', '<', '>']) && !html.starts_with(['=', '-', '+']) {
        return Cow::Borrowed(html);
    }
    let mut escaped = vec![];

    let bytes = html.as_bytes();

    if matches!(bytes, [b'=' | b'-' | b'+', ..]) {
        escaped.push(b'\\');
    }

    for &ch in bytes {
        if matches!(ch, b'*' | b'_' | b'<' | b'>') {
            escaped.push(b'\\');
        }
        escaped.push(ch);
    }

    Cow::Owned(
        String::from_utf8(escaped)
            // SAFETY: we started with valid utf8
            .unwrap(),
    )
}
