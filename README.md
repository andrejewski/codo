# Codo
> The code comment TODO tool

Make TODO comments powerful with Codo, a command-line tool to search, manage, format, and validate your codebase's TODO comments.

## Installation

Codo is distributed as a Rust crate, so [Rustup](https://rustup.rs/) and then run:

```sh
cargo install codo
```

## TODO anatomy

Codo-style TODO comments have these shapes:

```rs
// TODO: Simple example with no metadata
// TODO(@chris): Example TODO assigned to "chris"
// TODO(#123): Example TODO citing Github-like issue "#123"
// TODO(PROJ-123): Example TODO citing a Jira-like issue
// TODO(2023-11-01): Example TODO with a due date of November 1st, 2023
// TODO(#123, @chris, 2023-11-01): Example of all three metadata pieces
```

Don't worry about the syntax too much though, `codo format` and `codo validate` as shown below will keep up the hygiene.

## Basic commands

### Search TODOs

```sh
# list all TODOs
codo list

# list all overdue TODOs
codo list --overdue

# list all unassigned TODOs
codo list --unassigned

# list all TODOs assigned to someone
codo list --assignee=chris
```

### Get TODO stats

```sh
# Get total TODO count 
codo stat

# Get TODO count by assignee
codo stat --group-by=assignee
```

### Format TODOs

```sh
codo format
```

This command rewrites TODO comments into proper form. For examples:

```
// TODO example
// todo example
// ToDo example
//    TODO: example
```

All get formatted to `// TODO: example`. Version control is highly recommended, especially when running this command as it modifies files in-place.

### Validate TODOs

Have TODO hygiene you'd like to enforce? This command is for you:

```sh
codo validate
  --require-assignees
  --require-due-dates
  --require-issues
```

This command will return a non-zero exit status and print out validation errors if there are any.
A great tool to add to your pre-push/commit hooks and CI pipeline to force consistency.

### Export TODOs

```sh
codo export json
```

Prints out JSON in this shape:

```json
{
  "version": "<current codo version>",
  "todos": [
    {
       "assignee": "chris",
        "due": "2023-11-01",
        "issue": "#123",
        "line": 42,
        "note": "This is an example",
        "path": "./README.md"
    }
  ]
}
```

### Various code mods

There are code mods you can use to manipulate TODOs. Some cool ones:

```sh
# Assign all TODOs which lack an issue with issue #123
codo mod add-issue-for-all-untracked --issue="#123"

# Bulk update TODO assignees
codo mod rename-assignee --from="old_name" --to="new_name"
```

