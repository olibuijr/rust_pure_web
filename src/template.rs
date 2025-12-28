//! Minimal template engine with layout support
use std::collections::HashMap;
use std::fs;
use crate::config;

/// Template context - props passed through layouts
pub struct Context {
    props: HashMap<String, CtxValue>,
}

enum CtxValue {
    Text(String),
    Raw(String),
}

impl Context {
    pub fn new() -> Self {
        Self { props: HashMap::new() }
    }

    pub fn set(&mut self, key: &str, value: &str) -> &mut Self {
        self.props.insert(key.to_string(), CtxValue::Text(value.to_string()));
        self
    }

    pub fn set_raw(&mut self, key: &str, value: &str) -> &mut Self {
        self.props.insert(key.to_string(), CtxValue::Raw(value.to_string()));
        self
    }

    /// Replace all {{ key }} with values
    pub fn apply(&self, template: &str) -> String {
        let mut result = template.to_string();
        for (key, value) in &self.props {
            let rendered = match value {
                CtxValue::Text(v) => escape_html(v),
                CtxValue::Raw(v) => v.clone(),
            };
            result = result.replace(&format!("{{{{ {} }}}}", key), &rendered);
        }
        result
    }
}

/// Load template file
pub fn load(name: &str) -> String {
    fs::read_to_string(config::templates_dir().join(name))
        .unwrap_or_else(|_| format!("<!-- Template not found: {} -->", name))
}

/// Process {% include "file.html" %} directives
pub fn process_includes(template: &str) -> String {
    let mut result = template.to_string();
    let mut depth = 0;
    while depth < 20 {
        let Some(start) = result.find("{% include \"") else { break; };
        let quote_start = start + 12;
        let Some(quote_end) = result[quote_start..].find("\" %}") else { break; };
        let filename = &result[quote_start..quote_start + quote_end];
        let tag_end = quote_start + quote_end + 4;
        let content = load(filename);
        result = format!("{}{}{}", &result[..start], content, &result[tag_end..]);
        depth += 1;
    }
    result
}

/// Render template with layout support and context
/// Use {% layout "layouts/name.html" %} to wrap in a layout
/// Layout uses {{ children }} for content insertion
pub fn render(content: &str, ctx: &Context) -> String {
    let result = process_includes(content);

    // Check for {% layout "..." %}
    if let Some(start) = result.find("{% layout \"") {
        let quote_start = start + 11;
        if let Some(quote_end) = result[quote_start..].find("\" %}") {
            let layout_name = &result[quote_start..quote_start + quote_end];
            let tag_end = quote_start + quote_end + 4;

            // Get content after layout tag, apply context
            let inner = ctx.apply(result[tag_end..].trim());

            // Load and recursively render layout
            let layout = render(&load(layout_name), ctx);

            return layout.replace("{{ children }}", &inner);
        }
    }

    ctx.apply(&result)
}

pub fn process_conditionals(mut html: String, key: &str, enabled: bool) -> String {
    let tag = format!("{{% if {} %}}", key);
    let end_tag = "{% endif %}";
    while let Some(start) = html.find(&tag) {
        let tag_end = start + tag.len();
        if let Some(endif_offset) = html[tag_end..].find(end_tag) {
            let endif_start = tag_end + endif_offset;
            let else_pos = html[tag_end..endif_start].find("{% else %}");
            let replacement = match else_pos {
                Some(else_offset) => {
                    let else_start = tag_end + else_offset;
                    if enabled { &html[tag_end..else_start] }
                    else { &html[else_start + 10..endif_start] }
                }
                None => if enabled { &html[tag_end..endif_start] } else { "" }
            };
            html = format!("{}{}{}", &html[..start], replacement, &html[endif_start + end_tag.len()..]);
        } else { break; }
    }
    html
}

pub fn process_loop<T, F>(html: &str, item: &str, items: &[T], get_fields: F) -> String
where
    F: Fn(&T) -> Vec<(&str, &str)>,
{
    let tag = format!("{{% for {} in {}s %}}", item, item);
    if let Some(start) = html.find(&tag) {
        let body_start = start + tag.len();
        if let Some(end_offset) = html[body_start..].find("{% endfor %}") {
            let body = &html[body_start..body_start + end_offset];
            let mut rendered = String::new();

            for i in items {
                let mut part = body.to_string();
                for (k, v) in get_fields(i) {
                    part = part.replace(&format!("{{{{ {}.{} }}}}", item, k), v);
                }
                rendered.push_str(&part);
            }

            return format!("{}{}{}", &html[..start], rendered, &html[body_start + end_offset + 12..]);
        }
    }
    html.to_string()
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
