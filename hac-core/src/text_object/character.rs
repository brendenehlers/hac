#[derive(PartialEq, Debug)]
pub enum Kind {
    Word,
    Whitespace,
    Punctuation,
}

pub fn kind(c: char, bigword: &bool) -> Kind {
    match c {
        _ if c.is_alphanumeric() => Kind::Word,
        _ if c.is_whitespace() => Kind::Whitespace,
        _ if *bigword => Kind::Word,
        _ => Kind::Punctuation,
    }
}

pub fn is_opening_token(char: char) -> bool {
    matches!(char, '(' | '{' | '[' | '<')
}

pub fn is_closing_token(char: char) -> bool {
    matches!(char, ')' | '}' | ']' | '>')
}
