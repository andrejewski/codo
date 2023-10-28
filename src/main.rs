use std::env;
use std::path::PathBuf;

use grep::matcher::{Captures, Matcher};
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::Searcher;
use ignore::Walk;
use regex::Regex;

// TODO: cake
// TODO: TODO
// TODO: This is a multi word TODO
// TODO(data): Example with data
// TODO(@me): Example with assignee
// TODO(#123): Example with ticket

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
                let metadata = TodoMetadata::from_string(meta.clone());

                let mut info: Vec<String> = vec![];
                if let Some(ticket) = metadata.ticket {
                    info.push(format!("#{}", ticket))
                }

                if let Some(assignee) = metadata.assignee {
                    info.push(format!("@{}", assignee))
                }

                if let Some(due) = metadata.due {
                    info.push(format!("due:{}", due))
                }

                let meta_part = if info.is_empty() {
                    meta
                } else {
                    info.join(", ")
                };

                format!(
                    "{}:{} [{}] {}",
                    self.path.display(),
                    self.line_number,
                    meta_part,
                    self.note
                )
            }
            None => {
                format!("{}:{} {}", self.path.display(), self.line_number, self.note)
            }
        }
    }
}

struct TodoMetadata {
    assignee: Option<String>,
    ticket: Option<String>,
    due: Option<String>,
}

impl TodoMetadata {
    fn from_string(str: String) -> Self {
        let date_format = Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}").unwrap();

        let mut assignee: Option<String> = None;
        let mut ticket: Option<String> = None;
        let mut due: Option<String> = None;

        let parts: Vec<&str> = str.trim().split(',').map(|s| s.trim()).collect();
        for part in parts {
            if part.starts_with('@') && assignee == None {
                assignee = Some(part[1..].to_string());
                continue;
            }

            if part.starts_with('#') && ticket == None {
                ticket = Some(part[1..].to_string());
                continue;
            }

            if date_format.is_match(part) && due == None {
                due = Some(part.to_string())
            }
        }

        TodoMetadata {
            assignee,
            ticket,
            due,
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
