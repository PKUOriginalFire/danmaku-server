use std::borrow::Cow;

use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
enum CQSegment<'a> {
    #[regex(r#"\[CQ:at,([^]]+)\]"#)]
    At(&'a str),
    #[regex(r#"[^\[]+"#)]
    #[regex(r#"\["#)]
    Text(&'a str),
    #[regex(r#"\[CQ:([^]]+)\]"#)]
    Unknown,
}

macro_rules! get_cq_arg {
    ($code:expr, $name:literal) => {
        $code
            .split_once(concat!(",", $name, "="))
            .and_then(|(_, v)| v.split_once([',', ']']))
            .map(|(v, _)| htmlize::unescape(v))
    };
}

impl<'a> CQSegment<'a> {
    pub fn to_text(&self) -> Cow<'a, str> {
        match self {
            CQSegment::At(at) => {
                if let Some(name) = get_cq_arg!(at, "name") {
                    format!("@{}", name).into()
                } else if let Some(qq) = get_cq_arg!(at, "qq") {
                    format!("@{}", qq).into()
                } else {
                    "".into()
                }
            }
            &CQSegment::Text(text) => text.into(),
            CQSegment::Unknown => "".into(),
        }
    }
}

pub fn cq_to_text(cqcode: &str) -> Vec<Cow<str>> {
    CQSegment::lexer(cqcode)
        .flatten()
        .map(|s| s.to_text())
        .collect()
}
