// Forked from https://github.com/risinglightdb/sqllogictest-rs/tree/main/sqllogictest-bin/src
mod db;
mod pg;
mod space;
mod sqlite;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::db::DBRunner;
use crate::pg::Pg;
use crate::space::SpaceDb;
use crate::sqlite::Sqlite;
use anyhow::{anyhow, bail, Context};
use chrono::Local;
use clap::{Parser, ValueEnum};
use console::style;
use itertools::Itertools;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestSuite};
use spacetimedb::error::DBError;
use sqllogictest::{
    default_validator, strict_column_validator, update_record_with_output, AsyncDB, Injected, MakeConnection, Record,
    Runner,
};

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "PascalCase")]
enum DbType {
    #[default]
    SpacetimeDB,
    Sqlite,
    Postgres,
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[must_use]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Parser, Debug, Clone)]
#[clap(about, version, author)]
struct Args {
    /// Glob(s) of a set of test files.
    /// Example: `./test/**/*.slt`
    #[clap(required = true, num_args = 1..)]
    files: Vec<String>,

    /// The database engine name.
    #[clap(short, long, value_enum, default_value = "SpacetimeDB")]
    engine: DbType,

    /// Whether to enable colors in the output.
    #[clap(long, value_enum, default_value_t, value_name = "WHEN")]
    color: Color,

    /// Whether to enable parallel test. One database will be created for each test file.
    #[clap(long, short)]
    jobs: Option<usize>,

    /// Overrides the test files with the actual output of the database.
    #[clap(long)]
    r#override: bool,
    /// Reformat the test files.
    #[clap(long)]
    format: bool,
}

const TEST_NAME: &str = "sqllogictest";

async fn flush(out: &mut impl std::io::Write) -> std::io::Result<()> {
    tokio::task::block_in_place(|| out.flush())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.color {
        Color::Always => {
            console::set_colors_enabled(true);
            console::set_colors_enabled_stderr(true);
        }
        Color::Never => {
            console::set_colors_enabled(false);
            console::set_colors_enabled_stderr(false);
        }
        Color::Auto => {}
    }

    let glob_patterns = args.files;
    let mut files: Vec<PathBuf> = Vec::new();
    for glob_pattern in glob_patterns.into_iter() {
        let pathbufs = glob::glob(&glob_pattern).context("failed to read glob pattern")?;
        for pathbuf in pathbufs.filter_map(Result::ok) {
            files.push(pathbuf)
        }
    }

    if files.is_empty() {
        bail!("no test case found");
    }
    if args.r#override || args.format {
        return update_test_files(files, args.engine, args.format).await;
    }

    let mut report = Report::new(TEST_NAME.to_string());
    report.set_timestamp(Local::now());

    let mut test_suite = TestSuite::new(TEST_NAME);
    test_suite.set_timestamp(Local::now());

    let result = run_serial(&mut test_suite, files, args.engine).await;

    report.add_test_suite(test_suite);

    result
}

async fn open_db(engine: DbType) -> Result<DBRunner, DBError> {
    Ok(match engine {
        DbType::SpacetimeDB => SpaceDb::new()?.into_db(),
        DbType::Sqlite => Sqlite::new().map_err(DBError::Other)?.into_db(),
        DbType::Postgres => Pg::new().await.map_err(DBError::Other)?.into_db(),
    })
}

/// Run test one by one
async fn run_serial(test_suite: &mut TestSuite, files: Vec<PathBuf>, engine: DbType) -> anyhow::Result<()> {
    let mut failed_case = vec![];
    let start = Instant::now();

    for file in files {
        let db_name = format!("{engine:?}");
        let runner = Runner::new(|| open_db(engine));
        let filename = file.to_string_lossy().to_string();
        let test_case_name = filename.replace(['/', ' ', '.', '-'], "_");
        let case = match run_test_file(&mut std::io::stdout(), runner, &file, &db_name).await {
            Ok((skipped, duration)) => {
                let status = if skipped {
                    TestCaseStatus::skipped()
                } else {
                    TestCaseStatus::success()
                };

                let mut case = TestCase::new(test_case_name, status);

                case.set_time(duration);
                case.set_timestamp(Local::now());
                case.set_classname(TEST_NAME);
                case
            }
            Err(e) => {
                println!("{}\n\n{:?}", style("[FAILED]").red().bold(), e);
                println!();
                failed_case.push(filename.clone());
                let mut status = TestCaseStatus::non_success(NonSuccessKind::Failure);
                status.set_type("test failure");
                let mut case = TestCase::new(test_case_name, status);
                case.set_timestamp(Local::now());
                case.set_classname(TEST_NAME);
                case.set_system_err(e.to_string());
                case.set_time(Duration::from_millis(0));
                case.set_system_out("");
                case
            }
        };
        test_suite.add_test_case(case);
    }

    if !failed_case.is_empty() {
        println!("some test case failed:\n{failed_case:#?}");
    }
    println!();

    let total = test_suite.tests - test_suite.disabled;
    let failed = test_suite.failures + test_suite.errors;
    let ok = total - failed;
    println!("{}: {}", style("[FAILED]").red().bold(), failed);
    println!("{}: {}", style("[SKIP  ]").yellow().bold(), test_suite.disabled);
    println!("{}: {}", style("[OK    ]").green().bold(), ok);
    println!("{}: {}%", style("[PASS  ]").blue().bold(), (ok * 100) / total);
    println!("{}: {:?}", style("[Elapsed]").reverse().bold(), start.elapsed());

    Ok(())
}

/// Different from [`Runner::run_file_async`], we re-implement it here to print some progress
/// information.
async fn run_test_file<T: std::io::Write, D: AsyncDB, M: MakeConnection<Conn = D>>(
    out: &mut T,
    mut runner: Runner<D, M>,
    filename: impl AsRef<Path>,
    db_name: &str,
) -> anyhow::Result<(bool, Duration)> {
    let filename = filename.as_ref();
    let records = sqllogictest::parse_file(filename).map_err(|e| anyhow!("{e:?}"))?;

    let mut begin_times = vec![];
    let mut did_pop = false;
    let mut skipped = 0;
    let mut total = 0;

    writeln!(out, "{: <60} .. ", filename.to_string_lossy())?;
    flush(out).await?;

    begin_times.push(Instant::now());

    for record in records {
        match &record {
            Record::Injected(Injected::BeginInclude(file)) => {
                begin_times.push(Instant::now());
                if !did_pop {
                    writeln!(out, "{}", style("[BEGIN]").blue().bold())?;
                } else {
                    writeln!(out)?;
                }
                did_pop = false;
                write!(out, "{}{: <60} .. ", "| ".repeat(begin_times.len() - 1), file)?;
                flush(out).await?;
            }
            Record::Injected(Injected::EndInclude(file)) => {
                finish_test_file(out, &mut begin_times, &mut did_pop, file, total - skipped == 0).await?;
            }
            _ => {}
        }

        let will_skip = match &record {
            Record::Statement { conditions, .. } | Record::Query { conditions, .. } => {
                total += 1;
                conditions.iter().any(|c| should_skip(c, [db_name]))
            }
            _ => false,
        };

        if will_skip {
            skipped += 1;
        }

        runner
            .run_async(record)
            .await
            .map_err(|e| anyhow!("{}", e.display(console::colors_enabled())))
            .context(format!("failed to run `{}`", style(filename.to_string_lossy()).bold()))?;
    }

    let duration = begin_times[0].elapsed();

    let skipped = total - skipped == 0;

    finish_test_file(
        out,
        &mut begin_times,
        &mut did_pop,
        &filename.to_string_lossy(),
        skipped,
    )
    .await?;

    writeln!(out)?;

    Ok((skipped, duration))
}

// sqllogictest::Condition::should_skip
fn should_skip<'a>(c: &'a sqllogictest::Condition, labels: impl IntoIterator<Item = &'a str>) -> bool {
    match c {
        sqllogictest::Condition::OnlyIf { label } => !labels.into_iter().contains(&label.as_str()),
        sqllogictest::Condition::SkipIf { label } => labels.into_iter().contains(&label.as_str()),
    }
}

async fn finish_test_file<T: std::io::Write>(
    out: &mut T,
    time_stack: &mut Vec<Instant>,
    did_pop: &mut bool,
    file: &str,
    skipped: bool,
) -> anyhow::Result<()> {
    let begin_time = time_stack.pop().unwrap();
    let result = if skipped {
        style("[SKIP]").yellow().strikethrough().bold()
    } else {
        style("[OK]").green().bold()
    };

    if *did_pop {
        // start a new line if the result is not immediately after the item
        write!(
            out,
            "\n{}{} {: <54} .. {} in {} ms",
            "| ".repeat(time_stack.len()),
            style("[END]").blue().bold(),
            file,
            result,
            begin_time.elapsed().as_millis()
        )?;
    } else {
        // otherwise, append time to the previous line
        write!(out, "{} in {} ms", result, begin_time.elapsed().as_millis())?;
    }

    *did_pop = true;

    Ok::<_, anyhow::Error>(())
}

/// * `format` - If true, will not run sqls, only formats the file.
async fn update_test_files(files: Vec<PathBuf>, engine: DbType, format: bool) -> anyhow::Result<()> {
    for file in files {
        let runner = Runner::new(|| open_db(engine));

        if let Err(e) = update_test_file(&mut std::io::stdout(), runner, &file, format).await {
            {
                println!("{}\n\n{:?}", style("[FAILED]").red().bold(), e);
                println!();
            }
        };
    }

    Ok(())
}

/// Different from [`sqllogictest::update_test_file`], we re-implement it here to print some
/// progress information.
async fn update_test_file<T: std::io::Write, D: AsyncDB, M: MakeConnection<Conn = D>>(
    out: &mut T,
    mut runner: Runner<D, M>,
    filename: impl AsRef<Path>,
    format: bool,
) -> anyhow::Result<()> {
    let filename = filename.as_ref();
    let records = tokio::task::block_in_place(|| sqllogictest::parse_file(filename).map_err(|e| anyhow!("{e:?}")))
        .context("failed to parse sqllogictest file")?;

    let mut begin_times = vec![];
    let mut did_pop = false;

    write!(out, "{: <60} .. ", filename.to_string_lossy())?;
    flush(out).await?;

    begin_times.push(Instant::now());

    fn create_outfile(filename: impl AsRef<Path>) -> std::io::Result<(PathBuf, File)> {
        let filename = filename.as_ref();
        let outfilename = filename.file_name().unwrap().to_str().unwrap().to_owned() + ".temp";
        let outfilename = filename.parent().unwrap().join(outfilename);
        // create a temp file in read-write mode
        let outfile = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .read(true)
            .open(&outfilename)?;
        Ok((outfilename, outfile))
    }

    fn override_with_outfile(filename: &String, outfilename: &PathBuf, outfile: &mut File) -> std::io::Result<()> {
        // check whether outfile ends with multiple newlines, which happens if
        // - the last record is statement/query
        // - the original file ends with multiple newlines

        const N: usize = 8;
        let mut buf = [0u8; N];
        loop {
            outfile.seek(SeekFrom::End(-(N as i64))).unwrap();
            outfile.read_exact(&mut buf).unwrap();
            let num_newlines = buf.iter().rev().take_while(|&&b| b == b'\n').count();
            assert!(num_newlines > 0);

            if num_newlines > 1 {
                // if so, remove the last ones
                outfile
                    .set_len(outfile.metadata().unwrap().len() - num_newlines as u64 + 1)
                    .unwrap();
            }

            if num_newlines == 1 || num_newlines < N {
                break;
            }
        }

        fs_err::rename(outfilename, filename)?;

        Ok(())
    }

    struct Item {
        filename: String,
        outfilename: PathBuf,
        outfile: File,
        halt: bool,
    }
    let (outfilename, outfile) = create_outfile(filename)?;
    let mut stack = vec![Item {
        filename: filename.to_string_lossy().to_string(),
        outfilename,
        outfile,
        halt: false,
    }];

    for record in records {
        let Item {
            filename,
            outfilename,
            outfile,
            halt,
        } = stack.last_mut().unwrap();

        match &record {
            Record::Injected(Injected::BeginInclude(filename)) => {
                let (outfilename, outfile) = create_outfile(filename)?;
                stack.push(Item {
                    filename: filename.clone(),
                    outfilename,
                    outfile,
                    halt: false,
                });

                begin_times.push(Instant::now());
                if !did_pop {
                    writeln!(out, "{}", style("[BEGIN]").blue().bold())?;
                } else {
                    writeln!(out)?;
                }
                did_pop = false;
                write!(out, "{}{: <60} .. ", "| ".repeat(begin_times.len() - 1), filename)?;
                flush(out).await?;
            }
            Record::Injected(Injected::EndInclude(file)) => {
                override_with_outfile(filename, outfilename, outfile)?;
                stack.pop();
                finish_test_file(out, &mut begin_times, &mut did_pop, file, false).await?;
            }
            _ => {
                if *halt {
                    writeln!(outfile, "{record}")?;
                    continue;
                }
                if matches!(record, Record::Halt { .. }) {
                    *halt = true;
                    writeln!(outfile, "{record}")?;
                    continue;
                }
                match &record {
                    Record::Statement { sql, .. } => {
                        if sql.contains("NOT_REWRITE") {
                            continue;
                        }
                    }
                    Record::Query { sql, .. } => {
                        if sql.contains("NOT_REWRITE") {
                            continue;
                        }
                    }
                    _ => (),
                }
                update_record(outfile, &mut runner, record, format)
                    .await
                    .context(format!("failed to run `{}`", style(filename).bold()))?;
            }
        }
    }

    finish_test_file(out, &mut begin_times, &mut did_pop, &filename.to_string_lossy(), false).await?;

    let Item {
        filename,
        outfilename,
        outfile,
        halt: _,
    } = stack.last_mut().unwrap();
    override_with_outfile(filename, outfilename, outfile)?;

    Ok(())
}

async fn update_record<D: AsyncDB, M: MakeConnection<Conn = D>>(
    outfile: &mut File,
    runner: &mut Runner<D, M>,
    record: Record<D::ColumnType>,
    format: bool,
) -> anyhow::Result<()> {
    assert!(!matches!(record, Record::Injected(_)));

    if format {
        writeln!(outfile, "{record}")?;
        return Ok(());
    }

    let record_output = runner.apply_record(record.clone()).await;
    match update_record_with_output(
        &record,
        &record_output,
        "\t",
        default_validator,
        strict_column_validator,
    ) {
        Some(new_record) => {
            writeln!(outfile, "{new_record}")?;
        }
        None => {
            writeln!(outfile, "{record}")?;
        }
    }

    Ok(())
}
