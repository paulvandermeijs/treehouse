//! Folding a turn's steps into one summary line, as the design's collapsed
//! steps board has it. Everything between the user's message and Pi's last
//! reply (thinking, tool calls and the notes Pi writes between them) folds
//! once Pi has moved on; what needs the user, cards and errors never do.

use std::collections::BTreeSet;
use std::ops::Range;

use crate::transcript::{Item, ToolCall, ToolStatus};

/// Fewer steps than this stay as they are: a summary wouldn't be shorter.
const MIN_FOLD: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Items drawn as they are.
    Items(Range<usize>),
    Fold(Fold),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    pub range: Range<usize>,
    /// The turn is still running: "Working for" rather than "Worked for".
    pub working: bool,
}

/// Cards for something with a life of its own split the steps.
fn is_card(tool: &ToolCall) -> bool {
    matches!(
        tool.name.as_str(),
        "subagent" | "treehouse_worktree" | "treehouse_merge"
    )
}

/// Whether an item can fold into a summary.
fn is_step(item: &Item) -> bool {
    match item {
        Item::Thinking { .. } | Item::Text { .. } => true,
        // A call waiting for approval needs the user.
        Item::Tool(tool) => !is_card(tool) && tool.status != ToolStatus::AwaitingApproval,
        _ => false,
    }
}

fn starts_turn(item: &Item) -> bool {
    matches!(item, Item::User { .. } | Item::Handover { .. })
}

fn is_live(item: &Item) -> bool {
    match item {
        Item::Text { streaming, .. } | Item::Thinking { streaming, .. } => *streaming,
        Item::Tool(tool) => matches!(tool.status, ToolStatus::Running | ToolStatus::Preparing),
        _ => false,
    }
}

/// Splits the transcript into items to draw as they are and folds.
/// `running` is whether Pi is working on the last turn.
pub fn segments(items: &[Item], running: bool) -> Vec<Segment> {
    // Runs of consecutive steps, and which turn each belongs to.
    let mut runs: Vec<(Range<usize>, usize)> = Vec::new();
    let mut turn = 0;
    let mut ix = 0;
    while ix < items.len() {
        if starts_turn(&items[ix]) {
            turn += 1;
        }
        if is_step(&items[ix]) {
            let start = ix;
            while ix < items.len() && is_step(&items[ix]) {
                ix += 1;
            }
            runs.push((start..ix, turn));
        } else {
            ix += 1;
        }
    }

    let mut out = Vec::new();
    let mut drawn = 0;
    for (n, (run, turn_of_run)) in runs.iter().enumerate() {
        let last_in_turn = runs.get(n + 1).is_none_or(|(_, t)| t != turn_of_run);
        let working = running && *turn_of_run == turn;
        let end = if working && run.end == items.len() {
            live_tail(items, run)
        } else if last_in_turn && !working {
            // The reply that ends the turn stays out.
            let mut end = run.end;
            while end > run.start && matches!(items[end - 1], Item::Text { .. }) {
                end -= 1;
            }
            end
        } else {
            run.end
        };
        if end - run.start < MIN_FOLD {
            continue;
        }
        if drawn < run.start {
            out.push(Segment::Items(drawn..run.start));
        }
        out.push(Segment::Fold(Fold {
            range: run.start..end,
            working,
        }));
        drawn = end;
    }
    if drawn < items.len() {
        out.push(Segment::Items(drawn..items.len()));
    }
    out
}

/// Where the part of a running turn's last run that stays in view starts:
/// the step running now and Pi's latest note just before it.
fn live_tail(items: &[Item], run: &Range<usize>) -> usize {
    let end = run.end;
    let note = |ix: usize| ix >= run.start && matches!(items[ix], Item::Text { .. });
    if is_live(&items[end - 1]) {
        if end >= 2 && note(end - 2) {
            end - 2
        } else {
            end - 1
        }
    } else if note(end - 1) {
        end - 1
    } else {
        end
    }
}

/// What a fold's summary line says.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// "read 6 files, edited 3, ran 4 commands"; empty with no tool calls.
    pub what: String,
    pub failed: usize,
    pub additions: usize,
    pub deletions: usize,
}

pub fn summarize(items: &[Item]) -> Summary {
    let mut read = BTreeSet::new();
    let mut edited = BTreeSet::new();
    let mut wrote = BTreeSet::new();
    let (mut commands, mut scripts, mut searches) = (0, 0, 0);
    // Other tools by how they're named, in the order they were first used.
    let mut used: Vec<(String, usize)> = Vec::new();
    let mut summary = Summary::default();
    for item in items {
        let Item::Tool(tool) = item else { continue };
        if tool.status == ToolStatus::Failed {
            summary.failed += 1;
        }
        let path = || tool.path().unwrap_or_default().to_string();
        let done = tool.status == ToolStatus::Done;
        match tool.name.as_str() {
            "read" => {
                read.insert(path());
            }
            "edit" | "write" => {
                if done {
                    if tool.name == "edit" {
                        edited.insert(path());
                    } else {
                        wrote.insert(path());
                    }
                    if let Some(patch) = tool.patch() {
                        let stat = crate::git::diff_stat(&patch);
                        summary.additions += stat.additions;
                        summary.deletions += stat.deletions;
                    }
                }
            }
            "bash" => commands += 1,
            "codemode" => scripts += 1,
            "grep" | "find" | "ls" | "tool_search" => searches += 1,
            _ => {
                let name = match tool.name.strip_prefix("mcp__") {
                    Some(rest) => rest.split("__").next().unwrap_or(rest).to_string(),
                    None => tool.clean_name().to_string(),
                };
                match used.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, count)) => *count += 1,
                    None => used.push((name, 1)),
                }
            }
        }
    }

    // What changed first, then what was looked at. Once one part has said
    // "files", the others leave the noun out.
    let mut parts = Vec::new();
    let mut said_files = false;
    let mut files = |verb: &str, count: usize, parts: &mut Vec<String>| {
        if count == 0 {
            return;
        }
        parts.push(if said_files {
            format!("{verb} {count}")
        } else {
            format!("{verb} {}", plural(count, "file", "files"))
        });
        said_files = true;
    };
    files("edited", edited.len(), &mut parts);
    files("wrote", wrote.len(), &mut parts);
    if commands > 0 {
        parts.push(format!("ran {}", plural(commands, "command", "commands")));
    }
    if scripts > 0 {
        parts.push(format!("ran {}", plural(scripts, "script", "scripts")));
    }
    for (name, count) in used {
        parts.push(format!("used {name} {}", plural(count, "time", "times")));
    }
    files("read", read.len(), &mut parts);
    if searches > 0 {
        parts.push(format!("searched {}", plural(searches, "time", "times")));
    }
    summary.what = parts.join(", ");
    summary
}

fn plural(count: usize, one: &str, many: &str) -> String {
    match count {
        1 if one == "time" => "once".to_string(),
        1 => format!("1 {one}"),
        _ => format!("{count} {many}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Fold, Segment, segments, summarize};
    use crate::transcript::{Item, ItemId, ToolCall, ToolStatus};
    use serde_json::json;
    use std::time::Instant;

    fn user(id: u64) -> Item {
        Item::User {
            id: ItemId(id),
            text: "Fix it".into(),
        }
    }

    fn text(id: u64, streaming: bool) -> Item {
        Item::Text {
            id: ItemId(id),
            text: "Note".into(),
            streaming,
        }
    }

    fn tool(id: u64, name: &str, path: &str, status: ToolStatus) -> Item {
        Item::Tool(ToolCall {
            id: ItemId(id),
            call_id: format!("c{id}"),
            name: name.into(),
            args: json!({ "path": path }),
            status,
            output: String::new(),
            details: json!(null),
            started: Instant::now(),
            duration: None,
            previous_content: None,
        })
    }

    fn done(id: u64, name: &str, path: &str) -> Item {
        tool(id, name, path, ToolStatus::Done)
    }

    #[test]
    fn folds_a_finished_turn_but_not_its_reply() {
        let items = [
            user(0),
            done(1, "read", "a"),
            text(2, false),
            done(3, "bash", ""),
            done(4, "read", "b"),
            text(5, false),
        ];
        assert_eq!(
            segments(&items, false),
            [
                Segment::Items(0..1),
                Segment::Fold(Fold {
                    range: 1..5,
                    working: false
                }),
                Segment::Items(5..6),
            ]
        );
    }

    #[test]
    fn short_turns_stay_as_they_are() {
        let items = [
            user(0),
            done(1, "read", "a"),
            done(2, "bash", ""),
            text(3, false),
        ];
        assert_eq!(segments(&items, false), [Segment::Items(0..4)]);
    }

    #[test]
    fn cards_and_approvals_split_the_steps() {
        let items = [
            user(0),
            done(1, "read", "a"),
            done(2, "read", "b"),
            done(3, "bash", ""),
            done(4, "subagent", ""),
            done(5, "bash", ""),
            tool(6, "bash", "", ToolStatus::AwaitingApproval),
        ];
        assert_eq!(
            segments(&items, true),
            [
                Segment::Items(0..1),
                Segment::Fold(Fold {
                    range: 1..4,
                    working: true
                }),
                Segment::Items(4..7),
            ]
        );
    }

    #[test]
    fn a_running_turn_keeps_its_note_and_current_step() {
        let items = [
            user(0),
            done(1, "read", "a"),
            done(2, "read", "b"),
            done(3, "edit", "c"),
            text(4, false),
            tool(5, "edit", "d", ToolStatus::Running),
        ];
        assert_eq!(
            segments(&items, true),
            [
                Segment::Items(0..1),
                Segment::Fold(Fold {
                    range: 1..4,
                    working: true
                }),
                Segment::Items(4..6),
            ]
        );
    }

    #[test]
    fn earlier_turns_fold_while_the_last_one_runs() {
        let items = [
            user(0),
            done(1, "read", "a"),
            done(2, "read", "b"),
            done(3, "bash", ""),
            text(4, false),
            user(5),
            text(6, true),
        ];
        assert_eq!(
            segments(&items, true),
            [
                Segment::Items(0..1),
                Segment::Fold(Fold {
                    range: 1..4,
                    working: false
                }),
                Segment::Items(4..7),
            ]
        );
    }

    #[test]
    fn summary_puts_changes_first_and_counts_failures() {
        let items = [
            done(1, "read", "a"),
            done(2, "read", "b"),
            done(3, "read", "a"),
            done(4, "edit", "c"),
            tool(5, "edit", "d", ToolStatus::Failed),
            done(6, "edit", "d"),
            done(7, "bash", ""),
            done(8, "mcp__figma__get_screenshot", ""),
            done(9, "mcp__figma__get_metadata", ""),
            done(10, "grep", ""),
        ];
        let summary = summarize(&items);
        assert_eq!(
            summary.what,
            "edited 2 files, ran 1 command, used figma 2 times, read 2, searched once"
        );
        assert_eq!(summary.failed, 1);
    }

    #[test]
    fn thinking_and_notes_alone_say_nothing() {
        assert_eq!(summarize(&[text(1, false), text(2, false)]).what, "");
    }
}
