// Text templates with `{placeholder}` substitution, used for the header.
// `{{` and `}}` produce literal braces. Unknown placeholders are kept as
// literal text so a typo shows up on screen instead of silently vanishing.

use log::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    Hostname
}

impl Placeholder {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "hostname" => Some(Placeholder::Hostname),
            _ => None
        }
    }

    fn resolve(self) -> std::io::Result<String> {
        match self {
            Placeholder::Hostname => {
                Ok(hostname::get()?.to_string_lossy().into_owned())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Placeholder(Placeholder)
}

#[derive(Debug, Clone)]
pub struct Template {
    segments: Vec<Segment>
}

impl Template {
    pub fn parse(src: &str) -> Self {
        let mut segments = Vec::new();
        let mut literal = String::new();
        let mut chars = src.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '{' => {
                    let mut name = String::new();
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if c == '}' {
                            closed = true;
                            break;
                        }
                        name.push(c);
                    }
                    match Placeholder::from_name(name.trim()) {
                        Some(p) if closed => {
                            if !literal.is_empty() {
                                segments.push(Segment::Literal(
                                    std::mem::take(&mut literal)
                                ));
                            }
                            segments.push(Segment::Placeholder(p));
                        }
                        _ => {
                            if closed {
                                warn!("Unknown placeholder {{{name}}} in template {src:?}");
                            } else {
                                warn!("Unterminated placeholder in template {src:?}");
                            }
                            literal.push('{');
                            literal.push_str(&name);
                            if closed {
                                literal.push('}');
                            }
                        }
                    }
                }
                c => literal.push(c)
            }
        }
        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Template { segments }
    }

    pub fn render(&self) -> std::io::Result<String> {
        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(s) => out.push_str(s),
                Segment::Placeholder(p) => out.push_str(&p.resolve()?)
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(src: &str) -> Vec<Segment> {
        Template::parse(src).segments
    }

    fn lit(s: &str) -> Segment {
        Segment::Literal(s.to_string())
    }

    #[test]
    fn placeholder() {
        assert_eq!(
            segments("Welcome to {hostname}!"),
            vec![
                lit("Welcome to "),
                Segment::Placeholder(Placeholder::Hostname),
                lit("!")
            ]
        );
    }

    #[test]
    fn escaped_braces() {
        assert_eq!(segments("{{hostname}}"), vec![lit("{hostname}")]);
    }

    #[test]
    fn unknown_placeholder_is_literal() {
        assert_eq!(segments("a {nope} b"), vec![lit("a {nope} b")]);
    }

    #[test]
    fn unterminated_placeholder_is_literal() {
        assert_eq!(segments("a {hostname"), vec![lit("a {hostname")]);
    }

    #[test]
    fn empty() {
        assert_eq!(segments(""), vec![]);
        assert_eq!(Template::parse("").render().unwrap(), "");
    }
}
