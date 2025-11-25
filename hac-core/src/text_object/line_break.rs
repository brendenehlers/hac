#[derive(Debug, PartialEq, Clone)]
pub enum LineBreak {
    Lf,
    Crlf,
}

impl From<LineBreak> for usize {
    fn from(value: LineBreak) -> usize {
        match value {
            LineBreak::Lf => 1,
            LineBreak::Crlf => 2,
        }
    }
}

impl std::fmt::Display for LineBreak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lf => f.write_str("\n"),
            Self::Crlf => f.write_str("\r\n"),
        }
    }
}
