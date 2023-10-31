use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use grep::matcher::{Captures, Matcher};
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::Searcher;
use ignore::Walk;
use regex::Regex;

struct Todo {
    path: PathBuf,
    line_number: u64,
    note: String,
    meta: Option<String>,
    metadata: TodoMetadata,
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
    fn empty() -> Self {
        TodoMetadata {
            assignee: None,
            ticket: None,
            due: None,
        }
    }

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

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

enum Grouping {
    Assignee,
    Due,
    Ticket,
}

impl Grouping {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "assignee" => Some(Grouping::Assignee),
            "due" => Some(Grouping::Due),
            "ticket" => Some(Grouping::Ticket),
            _ => None,
        }
    }
}

struct TodoFilters {
    assignee: Option<Vec<String>>,
    unassigned: bool,

    ticket: Option<Vec<String>>,
    untracked: bool,

    due: Option<Vec<String>>,
    overdue: bool,
    someday: bool,
}

#[derive(Subcommand)]
enum Commands {
    List {
        #[arg(long)]
        assignee: Option<Vec<String>>,

        #[arg(long)]
        unassigned: bool,

        #[arg(long)]
        ticket: Option<Vec<String>>,

        #[arg(long)]
        untracked: bool,

        #[arg(long)]
        due: Option<Vec<String>>,

        #[arg(long)]
        overdue: bool,

        #[arg(long)]
        someday: bool,
    },
    Stat {
        #[arg(long)]
        assignee: Option<Vec<String>>,

        #[arg(long)]
        unassigned: bool,

        #[arg(long)]
        ticket: Option<Vec<String>>,

        #[arg(long)]
        untracked: bool,

        #[arg(long)]
        due: Option<Vec<String>>,

        #[arg(long)]
        overdue: bool,

        #[arg(long)]
        someday: bool,

        #[arg(long)]
        group_by: Option<String>,
    },
    Mod {
        #[command(subcommand)]
        code_mod: CodeMod,
    },
}

#[derive(Subcommand)]
enum CodeMod {
    RenameTicket {
        #[arg(long)]
        from: String,

        #[arg(long)]
        to: String,
    },

    AssignTicket {
        #[arg(long)]
        ticket: String,

        #[arg(long)]
        assignee: String,
    },
}

fn filter_by_match(
    value: Option<String>,
    selection: Option<Vec<String>>,
    include_unset: bool,
) -> bool {
    if let Some(list) = selection {
        if let Some(a) = value {
            list.contains(&a)
        } else {
            include_unset
        }
    } else if include_unset {
        value == None
    } else {
        true
    }
}

fn parse_due_date(date_str: String) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").ok()
}

fn filter_todo_list(list: Vec<Todo>, filters: TodoFilters) -> Vec<Todo> {
    list.into_iter()
        .filter(|todo| {
            filter_by_match(
                todo.metadata.assignee.to_owned(),
                filters.assignee.to_owned(),
                filters.unassigned,
            ) && filter_by_match(
                todo.metadata.ticket.to_owned(),
                filters.ticket.to_owned(),
                filters.untracked,
            ) && filter_by_match(
                todo.metadata.due.to_owned(),
                filters.due.to_owned(),
                filters.someday,
            ) && if filters.overdue {
                if let Some(due) = todo.metadata.due.to_owned() {
                    if let Some(date) = parse_due_date(due) {
                        date > Local::now().date_naive()
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                true
            }
        })
        .collect()
}

fn make_metadata_str(metadata: TodoMetadata) -> Option<String> {
    let mut parts: Vec<String> = vec![];
    if let Some(ticket) = metadata.ticket {
        parts.push(format!("#{}", ticket))
    }

    if let Some(assignee) = metadata.assignee {
        parts.push(format!("@{}", assignee))
    }

    if let Some(due) = metadata.due {
        parts.push(format!("{}", due))
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn apply_updates(updates: Vec<TodoUpdate>) {
    let mut file_updates: HashMap<PathBuf, HashMap<u64, TodoUpdate>> = HashMap::new();
    for update in updates.into_iter() {
        file_updates
            .entry(update.path.clone())
            .or_default()
            .insert(update.line_number - 1, update);
    }

    for (path, line_updates) in file_updates.borrow_mut() {
        if let Ok(handle) = File::open(path.clone()) {
            let mut output_lines: Vec<String> = vec![];

            let reader = BufReader::new(handle);
            for (num, line_result) in reader.lines().enumerate() {
                if let Ok(line) = line_result {
                    let new_line = if let Some(update) = line_updates.remove(&(num as u64)) {
                        let leading_whitespace = line.split("//").nth(0).unwrap_or("");
                        if let Some(meta) = make_metadata_str(update.metadata) {
                            format!("{}// TODO({}): {}", leading_whitespace, meta, update.note)
                        } else {
                            format!("{}// TODO: {}", leading_whitespace, update.note)
                        }
                    } else {
                        line
                    };

                    output_lines.push(new_line);
                }
            }

            if let Ok(mut new_file) = File::create(path) {
                let _ = new_file.write_all(output_lines.join("\n").as_bytes());
            }
        }
    }

    ()
}

struct TodoUpdate {
    path: PathBuf,
    line_number: u64,
    note: String,
    metadata: TodoMetadata,
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

                            let metadata = if let Some(meta_str) = meta.to_owned() {
                                TodoMetadata::from_string(meta_str)
                            } else {
                                TodoMetadata::empty()
                            };

                            match note {
                                Some(note) => {
                                    let todo = Todo {
                                        path: entry.path().to_path_buf(),
                                        line_number,
                                        note,
                                        meta,
                                        metadata,
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

    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Commands::List {
        assignee: None,
        ticket: None,
        due: None,
        untracked: false,
        unassigned: false,
        overdue: false,
        someday: false,
    });

    match command {
        Commands::Stat {
            assignee,
            unassigned,
            ticket,
            untracked,
            due,
            someday,
            overdue,
            group_by,
        } => {
            let results = filter_todo_list(
                matches,
                TodoFilters {
                    assignee,
                    unassigned,
                    ticket,
                    untracked,
                    due,
                    overdue,
                    someday,
                },
            );

            if let Some(group_by) = group_by {
                if let Some(grouping) = Grouping::from_str(&group_by) {
                    let mut map: HashMap<String, u32> = HashMap::new();

                    for todo in results {
                        let key = match grouping {
                            Grouping::Assignee => {
                                todo.metadata.assignee.unwrap_or("<unassigned>".to_string())
                            }
                            Grouping::Due => todo.metadata.due.unwrap_or("<someday>".to_string()),
                            Grouping::Ticket => {
                                todo.metadata.ticket.unwrap_or("<untracked>".to_string())
                            }
                        };

                        let count = map.get(&key).unwrap_or(&0);
                        map.insert(key, count + 1);
                    }

                    let mut entries: Vec<(String, u32)> = vec![];
                    for (key, value) in map {
                        entries.push((key, value));
                    }

                    entries.sort_by(|(_, a), (_, b)| b.cmp(a));

                    println!(
                        "{}",
                        entries
                            .iter()
                            .map(|(key, count)| format!("{}: {}", key, count))
                            .collect::<Vec<String>>()
                            .join("\n")
                    )
                } else {
                    println!("ERROR: --group-by={} not supported", { group_by })
                }
            } else {
                println!("{}", results.len())
            }
        }
        Commands::List {
            assignee,
            unassigned,
            ticket,
            untracked,
            due,
            someday,
            overdue,
        } => {
            let results = filter_todo_list(
                matches,
                TodoFilters {
                    assignee,
                    unassigned,
                    ticket,
                    untracked,
                    due,
                    overdue,
                    someday,
                },
            );

            if results.is_empty() {
                println!("<no TODOs>")
            } else {
                println!(
                    "{}",
                    results
                        .iter()
                        .map(|t| t.as_search_result())
                        .collect::<Vec<String>>()
                        .join("\n")
                );
            }
        }
        Commands::Mod { code_mod } => match code_mod {
            CodeMod::AssignTicket { ticket, assignee } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.ticket == Some(ticket.to_owned()))
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            assignee: Some(assignee.clone()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    println!("No TODOs matching ticket \"{}\"", ticket)
                } else {
                    apply_updates(updates);
                    println!(
                        "All TODOs with ticket \"{}\" assigned to \"{}\"",
                        ticket, assignee
                    )
                }
            }
            CodeMod::RenameTicket { from, to } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.ticket == Some(from.to_owned()))
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            ticket: Some(to.clone()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    println!("No TODOs matching ticket \"{}\"", from)
                } else {
                    apply_updates(updates);
                    println!("All TODOs with ticket \"{}\" assigned to \"{}\"", from, to)
                }
            }
        },
    }

    Ok(())
}
