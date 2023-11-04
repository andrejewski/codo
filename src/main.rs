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
use regex::Regex;

struct Todo {
    raw: String,
    delimiter: String,
    path: PathBuf,
    line_number: u64,
    note: String,
    meta: Option<String>,
    metadata: TodoMetadata,
}

impl Todo {
    fn as_search_result(&self) -> String {
        let note: String = if self.delimiter == "/*" {
            if let Some(stripped_note) = self.note.strip_suffix("*/") {
                stripped_note.to_string()
            } else {
                self.note.to_owned()
            }
        } else {
            self.note.to_owned()
        };

        match self.meta.to_owned() {
            Some(meta) => {
                let metadata = TodoMetadata::from_string(meta.clone());

                let mut info: Vec<String> = vec![];
                if let Some(issue) = metadata.issue {
                    info.push(format!("{}", issue.as_string()))
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
                    note
                )
            }
            None => {
                format!("{}:{} {}", self.path.display(), self.line_number, note)
            }
        }
    }
}

#[derive(Clone)]
struct TodoMetadata {
    assignee: Option<String>,
    issue: Option<Issue>,
    due: Option<String>,
}

enum IssueFormat {
    Numbered,
    ProjectKey,
}

impl IssueFormat {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "numbered" => Some(IssueFormat::Numbered),
            "project-key" => Some(IssueFormat::ProjectKey),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum Issue {
    Numbered(String),
    ProjectKey { project_key: String, number: String },
}

impl Issue {
    fn as_string(&self) -> String {
        match self {
            Issue::Numbered(s) => s.to_string(),
            Issue::ProjectKey {
                project_key,
                number,
            } => format!("{}-{}", project_key, number),
        }
    }
}

fn parse_issue(str: &str) -> Option<Issue> {
    if let Ok(numbered) = Regex::new(r"^#[[:digit:]]+$") {
        if numbered.is_match(&str) {
            return Some(Issue::Numbered(str.to_owned()));
        }
    }

    if let Ok(project_key) = Regex::new(r"^([A-Z][A-Z_0-9]*)-([[:digit:]]+)$") {
        let captures = project_key.captures(str)?;
        let (_, [project_key, number]) = captures.extract();

        return Some(Issue::ProjectKey {
            project_key: project_key.to_string(),
            number: number.to_string(),
        });
    }

    None
}

impl TodoMetadata {
    fn empty() -> Self {
        TodoMetadata {
            assignee: None,
            issue: None,
            due: None,
        }
    }

    fn from_string(str: String) -> Self {
        let date_format = Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}").unwrap();

        let mut assignee: Option<String> = None;
        let mut issue: Option<Issue> = None;
        let mut due: Option<String> = None;

        let parts: Vec<&str> = str.trim().split(',').map(|s| s.trim()).collect();
        for part in parts {
            if part.starts_with('@') && assignee == None {
                assignee = Some(part[1..].to_string());
                continue;
            }

            issue = issue.or_else(|| parse_issue(part));

            if date_format.is_match(part) && due == None {
                due = Some(part.to_string())
            }
        }

        TodoMetadata {
            assignee,
            issue,
            due,
        }
    }
}

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long)]
    path: Option<Vec<String>>,

    #[command(subcommand)]
    command: Option<Commands>,
}

enum Grouping {
    Assignee,
    Due,
    Issue,
}

impl Grouping {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "assignee" => Some(Grouping::Assignee),
            "due" => Some(Grouping::Due),
            "issue" => Some(Grouping::Issue),
            _ => None,
        }
    }
}

struct TodoFilters {
    assignee: Option<Vec<String>>,
    unassigned: bool,

    issue: Option<Vec<String>>,
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
        issue: Option<Vec<String>>,

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
        issue: Option<Vec<String>>,

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
    Lint {
        #[arg(long)]
        require_assignees: bool,

        #[arg(long)]
        require_issues: bool,

        #[arg(long)]
        require_due_dates: bool,

        #[arg(long)]
        allowed_assignees: Option<Vec<String>>,

        #[arg(long)]
        issue_format: Option<String>,

        #[arg(long)]
        issue_project_keys: Option<Vec<String>>,
    },
    Format,
    Mod {
        #[command(subcommand)]
        code_mod: CodeMod,
    },
}

#[derive(Subcommand)]
enum CodeMod {
    RemoveIssue {
        #[arg(long)]
        issue: String,
    },
    RemoveAllIssues,
    RenameIssue {
        #[arg(long)]
        from: String,

        #[arg(long)]
        to: String,
    },
    AddIssueForAllUntracked {
        #[arg(long)]
        issue: String,
    },

    RemoveAssignee {
        #[arg(long)]
        assignee: String,
    },
    RemoveAllAssignees,
    RenameAssignee {
        #[arg(long)]
        from: String,

        #[arg(long)]
        to: String,
    },
    AssignUnassigned {
        #[arg(long)]
        assignee: String,
    },

    AssignIssue {
        #[arg(long)]
        issue: String,

        #[arg(long)]
        assignee: String,
    },

    RemoveAllDueDates,
    AddMissingDueDates {
        #[arg(long)]
        date: String,
    },
    SetIssueDueDate {
        #[arg(long)]
        issue: String,

        #[arg(long)]
        date: String,
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
                todo.metadata.issue.as_ref().map(|i| i.as_string()),
                filters.issue.to_owned(),
                filters.untracked,
            ) && filter_by_match(
                todo.metadata.due.to_owned(),
                filters.due.to_owned(),
                filters.someday,
            ) && if filters.overdue {
                if let Some(due) = todo.metadata.due.to_owned() {
                    if let Some(date) = parse_due_date(due) {
                        date < Local::now().date_naive()
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
    if let Some(issue) = metadata.issue {
        parts.push(format!("{}", issue.as_string()))
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

fn format_todo_update(delimiter: &String, note: &String, metadata: TodoMetadata) -> String {
    if let Some(meta) = make_metadata_str(metadata) {
        format!("{} TODO({}): {}", delimiter, meta, note)
    } else {
        format!("{} TODO: {}", delimiter, note)
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
                        let leading_whitespace = line.split(&update.delimiter).nth(0).unwrap_or("");

                        format!(
                            "{}{}",
                            leading_whitespace,
                            format_todo_update(&update.delimiter, &update.note, update.metadata)
                        )
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
    delimiter: String,
    note: String,
    metadata: TodoMetadata,
}

struct LintErrorEntry {
    todo: Todo,
    errors: Vec<String>,
}

impl LintErrorEntry {
    fn as_cli_result(&self) -> String {
        let error_list = self
            .errors
            .to_owned()
            .into_iter()
            .map(|s| format!("\t- {}", s))
            .collect::<Vec<String>>()
            .join("\n");

        format!("{}\n{}", self.todo.as_search_result(), error_list)
    }
}

struct LintRules {
    require_assignees: bool,
    require_issues: bool,
    require_due_dates: bool,

    allowed_assignees: Option<Vec<String>>,
    issue_format: Option<IssueFormat>,
    issue_project_keys: Option<Vec<String>>,
}

fn get_lint_errors(todo: &Todo, lint_rules: &LintRules) -> Vec<String> {
    let mut errors = vec![];

    let formatted = format_todo_update(&todo.delimiter, &todo.note, todo.metadata.to_owned());
    if todo.raw != formatted {
        errors.push("Invalid format");
    }

    if lint_rules.require_assignees && todo.metadata.assignee.is_none() {
        errors.push("Missing assignee");
    }

    if let Some(allowed) = &lint_rules.allowed_assignees {
        if let Some(assignee) = &todo.metadata.assignee {
            if !allowed.contains(&assignee) {
                errors.push("Invalid assignee");
            }
        }
    }

    if lint_rules.require_issues && todo.metadata.issue.is_none() {
        errors.push("Missing issue");
    }

    if let Some(issue) = todo.metadata.issue.to_owned() {
        if let Some(format) = &lint_rules.issue_format {
            let valid_format = match (format, issue.to_owned()) {
                (IssueFormat::Numbered, Issue::Numbered(_)) => true,
                (IssueFormat::ProjectKey, Issue::ProjectKey { .. }) => true,
                _ => false,
            };

            if !valid_format {
                errors.push("Invalid issue format");
            }
        }

        if let Some(project_keys) = &lint_rules.issue_project_keys {
            if let Issue::ProjectKey { project_key, .. } = issue {
                if !project_keys.contains(&project_key) {
                    errors.push("Invalid project key");
                }
            }
        }
    }

    if lint_rules.require_due_dates && todo.metadata.due.is_none() {
        errors.push("Missing due date");
    }

    errors.into_iter().map(|s| s.to_owned()).collect()
}

fn cli_error(error: String) -> ! {
    eprintln!("{}", error);
    std::process::exit(1);
}

fn main() -> Result<(), ()> {
    let matcher = RegexMatcher::new(r"(?m)^\W*(//|/\*|#) (?:(?i)TODO)(?:\((.+)\))?:? (.+?)$")
        .map_err(|e| cli_error(format!("{}", e)))?;

    let mut matches: Vec<Todo> = vec![];
    let mut searcher = Searcher::new();

    let cli = Cli::parse();

    let mut paths = cli.path.unwrap_or(vec!["./".to_owned()]);
    let primary_path = paths.remove(0);
    let mut walk_builder = ignore::WalkBuilder::new(primary_path);
    for path in paths {
        walk_builder.add(path);
    }

    let walk = walk_builder.build();

    for result in walk {
        match result {
            Ok(entry) => {
                let is_file = entry
                    .file_type()
                    .and_then(|f| if f.is_file() { Some(()) } else { None });

                if is_file == None {
                    continue;
                }

                let search_result = searcher.search_path(
                    &matcher,
                    entry.path(),
                    UTF8(|line_number, line| {
                        let mut captures = matcher.new_captures()?;

                        let did_match = matcher.captures(line.as_bytes(), &mut captures)?;
                        if !did_match {
                            return Ok(true);
                        }

                        let delimiter_capture = captures.get(1);
                        let (raw, delimiter) = match delimiter_capture {
                            Some(delimiter_match) => (
                                line[delimiter_match.start()..(line.len() - 1)].to_owned(),
                                line[delimiter_match].to_string(),
                            ),
                            None => return Ok(true),
                        };

                        let meta_capture = captures.get(2);
                        let meta = match meta_capture {
                            Some(meta_match) => Some(line[meta_match].to_string()),
                            None => None,
                        };

                        let note_capture = captures.get(3);
                        let note = match note_capture {
                            Some(note_match) => line[note_match].to_string(),
                            None => return Ok(true),
                        };

                        let metadata = if let Some(meta_str) = meta.to_owned() {
                            TodoMetadata::from_string(meta_str)
                        } else {
                            TodoMetadata::empty()
                        };

                        let todo = Todo {
                            raw,
                            delimiter,
                            path: entry.path().to_path_buf(),
                            line_number,
                            note,
                            meta,
                            metadata,
                        };

                        matches.push(todo);

                        Ok(true)
                    }),
                );

                if let Err(err) = search_result {
                    cli_error(format!("{}", err));
                }
            }
            Err(err) => {
                cli_error(format!("{}", err));
            }
        }
    }

    let command = cli.command.unwrap_or(Commands::List {
        assignee: None,
        issue: None,
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
            issue,
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
                    issue,
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
                            Grouping::Issue => todo
                                .metadata
                                .issue
                                .map(|i| i.as_string())
                                .unwrap_or("<untracked>".to_string()),
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
                    cli_error(format!("--group-by={} not supported", group_by));
                }
            } else {
                println!("{}", results.len())
            }
        }
        Commands::List {
            assignee,
            unassigned,
            issue,
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
                    issue,
                    untracked,
                    due,
                    overdue,
                    someday,
                },
            );

            if results.is_empty() {
                cli_error("<no TODOs>".to_owned());
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
        Commands::Lint {
            require_assignees,
            require_issues,
            require_due_dates,
            allowed_assignees,
            issue_format,
            issue_project_keys,
        } => {
            let issue_format = if let Some(input_format) = issue_format {
                if let Some(valid_format) = IssueFormat::from_str(&input_format) {
                    Some(valid_format)
                } else {
                    cli_error(format!("Issue format invalid: \"{}\"", input_format));
                }
            } else {
                None
            };

            let lint_rules = LintRules {
                require_assignees,
                require_issues,
                require_due_dates,
                allowed_assignees,
                issue_format,
                issue_project_keys,
            };

            let lint_errors: Vec<LintErrorEntry> = matches
                .into_iter()
                .filter_map(|todo| {
                    let errors = get_lint_errors(&todo, &lint_rules);
                    if errors.is_empty() {
                        None
                    } else {
                        Some(LintErrorEntry { todo, errors })
                    }
                })
                .collect();

            if lint_errors.is_empty() {
                println!("Lint errors (0): Great job!")
            } else {
                cli_error(format!(
                    "Lint errors ({}):\n\n{}",
                    lint_errors.len(),
                    lint_errors
                        .into_iter()
                        .map(|t| t.as_cli_result())
                        .collect::<Vec<String>>()
                        .join("\n\n"),
                ));
            }
        }
        Commands::Format => {
            let updates: Vec<TodoUpdate> = matches
                .into_iter()
                .map(|item| TodoUpdate {
                    metadata: item.metadata,
                    note: item.note,
                    path: item.path,
                    line_number: item.line_number,
                    delimiter: item.delimiter,
                })
                .collect();

            if updates.is_empty() {
                cli_error("No TODOs found".to_owned());
            } else {
                apply_updates(updates);
                println!("TODOs formatted.")
            }
        }
        Commands::Mod { code_mod } => match code_mod {
            CodeMod::RemoveIssue { issue } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| {
                        todo.metadata.issue.as_ref().map(|i| i.as_string())
                            == Some(issue.to_owned())
                    })
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            issue: None,
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs citing issue \"{}\"", issue));
                } else {
                    apply_updates(updates);
                    println!("All citations of issue \"{}\" were removed.", issue)
                }
            }
            CodeMod::RemoveAllIssues => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| !todo.metadata.issue.is_none())
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            issue: None,
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs citing any issues".to_owned());
                } else {
                    apply_updates(updates);
                    println!("All citations of issues were removed.")
                }
            }
            CodeMod::RenameIssue { from, to } => {
                let to_issue = parse_issue(&to)
                    .ok_or_else(|| cli_error(format!("Invalid replacement issue \"{}\"", to)))?;

                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| {
                        todo.metadata.issue.as_ref().map(|i| i.as_string()) == Some(from.to_owned())
                    })
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            issue: Some(to_issue.to_owned()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs citing issue \"{}\"", from));
                } else {
                    apply_updates(updates);
                    println!("All TODOs citing issue \"{}\" assigned to \"{}\"", from, to)
                }
            }
            CodeMod::AddIssueForAllUntracked { issue } => {
                let valid_issue = parse_issue(&issue)
                    .ok_or_else(|| cli_error(format!("Invalid issue \"{}\"", issue)))?;

                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.issue.is_none())
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            issue: Some(valid_issue.to_owned()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs untracked".to_owned());
                } else {
                    apply_updates(updates);
                    println!("All untracked TODOs now cite issue \"{}\".", issue)
                }
            }
            CodeMod::RemoveAssignee { assignee } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.assignee == Some(assignee.to_owned()))
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            assignee: None,
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs assigned to \"{}\"", assignee));
                } else {
                    apply_updates(updates);
                    println!("All TODOs assigned to \"{}\" were unassigned.", assignee)
                }
            }
            CodeMod::RemoveAllAssignees => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.assignee != None)
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            assignee: None,
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs assigned".to_owned());
                } else {
                    apply_updates(updates);
                    println!("All TODOs were unassigned.")
                }
            }
            CodeMod::RenameAssignee { from, to } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.assignee == Some(from.to_owned()))
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            assignee: Some(to.clone()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs assigned to \"{}\"", from));
                } else {
                    apply_updates(updates);
                    println!(
                        "All TODOs assigned to \"{}\" were reassigned to \"{}\"",
                        from, to
                    )
                }
            }
            CodeMod::AssignUnassigned { assignee } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.assignee == None)
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
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs unassigned".to_owned());
                } else {
                    apply_updates(updates);
                    println!("All unassigned TODOs assigned to \"{}\"", assignee)
                }
            }
            CodeMod::AssignIssue { issue, assignee } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| {
                        todo.metadata.issue.as_ref().map(|i| i.as_string())
                            == Some(issue.to_owned())
                    })
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
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs citing issue \"{}\"", issue));
                } else {
                    apply_updates(updates);
                    println!(
                        "All TODOs citing issue \"{}\" assigned to \"{}\"",
                        issue, assignee
                    )
                }
            }
            CodeMod::RemoveAllDueDates => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.due != None)
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            due: None,
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs with due dates".to_owned());
                } else {
                    apply_updates(updates);
                    println!("All TODO due dates were removed.")
                }
            }
            CodeMod::AddMissingDueDates { date } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| todo.metadata.due == None)
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            due: Some(date.clone()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error("No TODOs without due dates".to_owned());
                } else {
                    apply_updates(updates);
                    println!(
                        "All TODO without due dates were set to be due \"{}\".",
                        date
                    )
                }
            }
            CodeMod::SetIssueDueDate { issue, date } => {
                let updates: Vec<TodoUpdate> = matches
                    .into_iter()
                    .filter(|todo| {
                        todo.metadata.issue.as_ref().map(|i| i.as_string())
                            == Some(issue.to_owned())
                    })
                    .map(|item| {
                        let new_metadata = TodoMetadata {
                            due: Some(date.clone()),
                            ..item.metadata
                        };

                        TodoUpdate {
                            metadata: new_metadata,
                            note: item.note,
                            path: item.path,
                            line_number: item.line_number,
                            delimiter: item.delimiter,
                        }
                    })
                    .collect();

                if updates.is_empty() {
                    cli_error(format!("No TODOs citing issue \"{}\"", issue));
                } else {
                    apply_updates(updates);
                    println!(
                        "All TODO citing issue \"{}\" to be due \"{}\".",
                        issue, date
                    )
                }
            }
        },
    }

    Ok(())
}
