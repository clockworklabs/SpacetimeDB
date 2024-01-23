use crate::api::{ClientApi, Connection};
use crate::sql::run_sql;
use colored::*;
use dirs::home_dir;
use std::env::temp_dir;

use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{Editor, Helper};

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

static SQL_SYNTAX: &str = include_str!("../../tools/sublime/SpacetimeDBSQL.sublime-syntax");
static SYNTAX_NAME: &str = "SQL (SpacetimeDB)";

static AUTO_COMPLETE: &str = "\
true
false
select
from
insert
into
values
update,
delete,
create,
where
join
sort by
.exit
.clear
";

pub async fn exec(con: Connection) -> Result<(), anyhow::Error> {
    let database = con.database.clone();
    let mut rl = Editor::<ReplHelper, DefaultHistory>::new().unwrap();
    let history = home_dir().unwrap_or_else(temp_dir).join(".stdb.history.txt");
    if rl.load_history(&history).is_err() {
        eprintln!("No previous history.");
    }
    rl.set_helper(Some(ReplHelper::new().unwrap()));

    println!(
        "\
┌──────────────────────────────────────────────────────────┐
│ .exit: Exit the REPL                                     │
│ .clear: Clear the Screen                                 │
│                                                          │
│ Give us feedback in our Discord server:                  │
│    https://discord.gg/w2DVqNZXdN                         │
└──────────────────────────────────────────────────────────┘",
    );

    let api = ClientApi::new(con);

    loop {
        let readline = rl.readline(&format!("🪐{}>", &database).green());
        match readline {
            Ok(line) => match line.as_str() {
                ".exit" => break,
                ".clear" => {
                    rl.clear_screen().ok();
                }
                sql => {
                    rl.add_history_entry(sql).ok();

                    if let Err(err) = run_sql(api.sql(), sql, true).await {
                        eprintln!("{}", err.to_string().red())
                    }
                }
            },
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("\n{}", "Aborted!".red());
                break;
            }
            x => {
                eprintln!("\nUnexpected: {x:?}");
                break;
            }
        }
    }

    rl.save_history(&history).ok();

    Ok(())
}

pub(crate) struct ReplHelper {
    syntaxes: SyntaxSet,
    theme: Theme,
    brackets: MatchingBracketValidator,
}

impl ReplHelper {
    pub fn new() -> Result<Self, ()> {
        let syntax_def = SyntaxDefinition::load_from_str(SQL_SYNTAX, false, Some(SYNTAX_NAME)).unwrap();
        let mut builder = SyntaxSetBuilder::new();
        builder.add(syntax_def);

        let syntaxes = builder.build();

        let _ps = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes["base16-ocean.dark"].clone();

        Ok(ReplHelper {
            syntaxes,
            theme,
            brackets: MatchingBracketValidator::new(),
        })
    }
}

impl Helper for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let mut name = String::new();
        let mut name_pos = pos;
        while let Some(char) = line
            .chars()
            .nth(name_pos.wrapping_sub(1))
            .filter(|c| c.is_ascii_alphanumeric() || ['_', '.'].contains(c))
        {
            name.push(char);
            name_pos -= 1;
        }
        if name.is_empty() {
            return Ok((0, vec![]));
        }
        name = name.chars().rev().collect();

        let completions: Vec<_> = AUTO_COMPLETE
            .split('\n')
            .filter(|it| it.starts_with(&name))
            .map(str::to_owned)
            .collect();

        Ok((name_pos, completions))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        if line.len() > pos {
            return None;
        }
        if let Ok((mut completion_pos, completions)) = self.complete(line, pos, ctx) {
            if completions.is_empty() {
                return None;
            }
            let mut hint = completions[0].clone();
            while completion_pos < pos {
                if hint.is_empty() {
                    return None;
                }
                hint.remove(0);
                completion_pos += 1;
            }
            Some(hint)
        } else {
            None
        }
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _: usize) -> std::borrow::Cow<'l, str> {
        let mut h = HighlightLines::new(self.syntaxes.find_syntax_by_name(SYNTAX_NAME).unwrap(), &self.theme);
        let mut out = String::new();
        for line in LinesWithEndings::from(line) {
            let ranges = h.highlight_line(line, &self.syntaxes).unwrap();
            let escaped = syntect::util::as_24_bit_terminal_escaped(&ranges[..], false);
            out += &escaped;
        }
        std::borrow::Cow::Owned(out)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _: bool) -> std::borrow::Cow<'b, str> {
        std::borrow::Cow::Owned(prompt.green().to_string())
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        std::borrow::Cow::Owned(hint.bright_black().to_string())
    }

    fn highlight_candidate<'c>(&self, candidate: &'c str, _: rustyline::CompletionType) -> std::borrow::Cow<'c, str> {
        std::borrow::Cow::Owned(candidate.bright_cyan().to_string())
    }

    fn highlight_char(&self, _: &str, _: usize) -> bool {
        true
    }
}

impl Validator for ReplHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.brackets.validate(ctx)
    }
}
