use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    CSharp,
    TypeScript,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::CSharp => "csharp",
            Lang::TypeScript => "typescript",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Lang::Rust => "Rust",
            Lang::CSharp => "C#",
            Lang::TypeScript => "TypeScript",
        }
    }
}

impl FromStr for Lang {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "rust" => Ok(Lang::Rust),
            "csharp" | "c#" | "cs" => Ok(Lang::CSharp),
            "typescript" | "ts" => Ok(Lang::TypeScript),
            other => Err(format!("unknown lang: {}", other)),
        }
    }
}
