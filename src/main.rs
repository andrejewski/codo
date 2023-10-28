use std::env;
use std::path::PathBuf;

use grep::matcher::{Captures, Matcher};
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::Searcher;
use ignore::Walk;

// TODO: cake
// TODO: TODO
// TODO: This is a multi word TODO
// TODO(data): Example with data

struct Todo {
    path: PathBuf,
    line_number: u64,
    note: String,
    meta: Option<String>,
}

impl Todo {
    fn as_search_result(&self) -> String {
        match self.meta.to_owned() {
            Some(meta) => {
                format!(
                    "{}:{} [{}] {}",
                    self.path.display(),
                    self.line_number,
                    meta,
                    self.note
                )
            }
            None => {
                format!("{}:{} {}", self.path.display(), self.line_number, self.note)
            }
        }
    }
}

fn main() -> Result<(), std::io::Error> {
    let matcher = match RegexMatcher::new(r"(?m)^\W*// TODO(?:\((.+)\))?: (.+)$") {
        Ok(matcher) => matcher,
        Err(error) => {
            println!("ERROR: {}", error);
            return Ok(());
        }
    };

    let mut matches: Vec<Todo> = vec![];
    let mut searcher = Searcher::new();

    for result in Walk::new("./") {
        match result {
            Ok(entry) => {
                match entry.file_type() {
                    Some(file_type) => {
                        if !file_type.is_file() {
                            continue;
                        }
                    }
                    None => {
                        continue;
                    }
                }

                searcher.search_path(
                    &matcher,
                    entry.path(),
                    UTF8(|line_number, line| {
                        let mut captures = matcher.new_captures()?;

                        let did_match = matcher.captures(line.as_bytes(), &mut captures)?;
                        if did_match {
                            let meta_capture = captures.get(1);
                            let meta = match meta_capture {
                                Some(meta_match) => Some(line[meta_match].to_string()),
                                None => None,
                            };

                            let note_capture = captures.get(2);
                            let note = match note_capture {
                                Some(note_match) => Some(line[note_match].to_string()),
                                None => None,
                            };

                            match note {
                                Some(note) => {
                                    let todo = Todo {
                                        path: entry.path().to_path_buf(),
                                        line_number,
                                        note,
                                        meta,
                                    };

                                    matches.push(todo);
                                }
                                None => {}
                            }
                        }

                        Ok(true)
                    }),
                )?;
            }
            Err(err) => println!("ERROR: {}", err),
        }
    }

    let args: Vec<String> = env::args().collect();
    let subcommand = match args.get(1) {
        Some(str) => str.as_str(),
        None => "list",
    };

    match subcommand {
        "stat" => {
            println!("TODO count: {}", matches.len())
        }
        "list" => {
            if matches.is_empty() {
                println!("No TODOs found. Great job!")
            } else {
                println!(
                    "{}",
                    matches
                        .iter()
                        .map(|t| t.as_search_result())
                        .collect::<Vec<String>>()
                        .join("\n")
                );
            }
        }
        _ => {
            println!("Unknown command {}", subcommand)
        }
    }

    Ok(())
}
