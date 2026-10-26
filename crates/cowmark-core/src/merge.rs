//! Line-based three-way merge. Overlapping changes are kept side by side
//! between standard git conflict markers, so git, plain editors and CowMark
//! all recognise them:
//!
//! ```text
//! <<<<<<< yours
//! the editor's lines
//! =======
//! the other lines
//! >>>>>>> theirs
//! ```

use similar::{capture_diff_slices, Algorithm, DiffOp, DiffTag};
use std::ops::Range;

pub const YOURS_MARKER: &str = "<<<<<<< yours";
pub const SEPARATOR_MARKER: &str = "=======";
pub const THEIRS_MARKER: &str = ">>>>>>> theirs";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    pub text: String,
    /// Number of conflict blocks written into `text`.
    pub conflicts: usize,
}

fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

struct Hunk {
    base: Range<usize>,
    side: Range<usize>,
    mine: bool,
}

fn hunks(base: &[&str], side: &[&str], mine: bool) -> Vec<Hunk> {
    capture_diff_slices(Algorithm::Myers, base, side)
        .into_iter()
        .filter(|op| op.tag() != DiffTag::Equal)
        .map(|op| Hunk { base: op.old_range(), side: op.new_range(), mine })
        .collect()
}

/// The side's lines that stand for `base[lo..hi]`, given that side's hunks in it.
fn side_region<'a>(side: &[&'a str], base: &[&'a str], lo: usize, hi: usize, group: &[&Hunk]) -> Vec<&'a str> {
    match (group.first(), group.last()) {
        (Some(first), Some(last)) => {
            let from = first.side.start - (first.base.start - lo);
            let to = last.side.end + (hi - last.base.end);
            side[from..to].to_vec()
        }
        _ => base[lo..hi].to_vec(),
    }
}

enum Piece<'a> {
    Same(Vec<&'a str>),
    Conflict(Vec<&'a str>, Vec<&'a str>),
}

fn is_blank(lines: &[&str]) -> bool {
    lines.iter().all(|line| line.trim().is_empty())
}

/// Splits a conflict where both sides share lines, so only the lines that
/// really differ are marked. Conflicts separated only by blank lines stay one.
fn refine<'a>(mine: Vec<&'a str>, theirs: Vec<&'a str>) -> Vec<Piece<'a>> {
    let mut pieces: Vec<Piece<'a>> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &mine, &theirs) {
        let (old, new) = (op.old_range(), op.new_range());
        match op {
            DiffOp::Equal { .. } => pieces.push(Piece::Same(mine[old].to_vec())),
            _ => match pieces.last_mut() {
                Some(Piece::Conflict(m, t)) => {
                    m.extend_from_slice(&mine[old]);
                    t.extend_from_slice(&theirs[new]);
                }
                _ => pieces.push(Piece::Conflict(mine[old].to_vec(), theirs[new].to_vec())),
            },
        }
    }

    let mut joined: Vec<Piece<'a>> = Vec::new();
    let mut iter = pieces.into_iter().peekable();
    while let Some(piece) = iter.next() {
        let blank_between = matches!(&piece, Piece::Same(same) if is_blank(same))
            && matches!(joined.last(), Some(Piece::Conflict(..)))
            && matches!(iter.peek(), Some(Piece::Conflict(..)));
        if blank_between {
            let Piece::Same(same) = piece else { unreachable!() };
            let Some(Piece::Conflict(next_mine, next_theirs)) = iter.next() else { unreachable!() };
            if let Some(Piece::Conflict(m, t)) = joined.last_mut() {
                m.extend(same.iter().copied().chain(next_mine));
                t.extend(same.iter().copied().chain(next_theirs));
            }
            continue;
        }
        joined.push(piece);
    }
    joined
}

fn push_block(out: &mut String, lines: &[&str], newline: &str) {
    for line in lines {
        out.push_str(line);
    }
    if lines.last().is_some_and(|line| !line.ends_with('\n')) {
        out.push_str(newline);
    }
}

/// Merges `mine` and `theirs`, both edited from `base`. Changes to different
/// lines are combined; changes to the same or touching lines become a conflict
/// block. Nothing from either side is dropped.
pub fn merge_text(base: &str, mine: &str, theirs: &str) -> Merged {
    if mine == theirs || base == theirs {
        return Merged { text: mine.to_string(), conflicts: 0 };
    }
    if base == mine {
        return Merged { text: theirs.to_string(), conflicts: 0 };
    }

    let newline = if mine.contains("\r\n") { "\r\n" } else { "\n" };
    let (base_lines, mine_lines, theirs_lines) = (lines(base), lines(mine), lines(theirs));
    let mut all = hunks(&base_lines, &mine_lines, true);
    all.extend(hunks(&base_lines, &theirs_lines, false));
    all.sort_by_key(|hunk| (hunk.base.start, hunk.base.end));

    let mut out = String::with_capacity(mine.len().max(theirs.len()));
    let mut conflicts = 0;
    let mut position = 0;
    let mut index = 0;
    while index < all.len() {
        let lo = all[index].base.start;
        let mut hi = all[index].base.end;
        let mut end = index + 1;
        // Touching changes (an insertion right at the edge of another change) conflict, as in git.
        while end < all.len() && all[end].base.start <= hi {
            hi = hi.max(all[end].base.end);
            end += 1;
        }
        let group = &all[index..end];
        index = end;

        out.extend(base_lines[position..lo].iter().copied());
        position = hi;

        let mine_group: Vec<&Hunk> = group.iter().filter(|hunk| hunk.mine).collect();
        let theirs_group: Vec<&Hunk> = group.iter().filter(|hunk| !hunk.mine).collect();
        let mine_region = side_region(&mine_lines, &base_lines, lo, hi, &mine_group);
        let theirs_region = side_region(&theirs_lines, &base_lines, lo, hi, &theirs_group);
        if theirs_group.is_empty() || mine_region == theirs_region {
            out.extend(mine_region);
            continue;
        }
        if mine_group.is_empty() {
            out.extend(theirs_region);
            continue;
        }

        for piece in refine(mine_region, theirs_region) {
            match piece {
                Piece::Same(same) => out.extend(same),
                Piece::Conflict(m, t) => {
                    conflicts += 1;
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push_str(newline);
                    }
                    out.push_str(YOURS_MARKER);
                    out.push_str(newline);
                    push_block(&mut out, &m, newline);
                    out.push_str(SEPARATOR_MARKER);
                    out.push_str(newline);
                    push_block(&mut out, &t, newline);
                    out.push_str(THEIRS_MARKER);
                    out.push_str(newline);
                }
            }
        }
    }
    out.extend(base_lines[position..].iter().copied());
    Merged { text: out, conflicts }
}

fn marker_line(line: &str, marker: char) -> bool {
    let line = line.trim_end_matches(['\r', '\n']);
    let rest = line.strip_prefix(&marker.to_string().repeat(7)).unwrap_or("x");
    rest.is_empty() || rest.starts_with(' ')
}

/// Whether `text` still has a conflict block, or a stray `<<<<<<<` / `>>>>>>>`
/// line left from one. A lone `=======` is a valid Markdown heading underline,
/// so it does not count by itself.
pub fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|line| marker_line(line, '<') || marker_line(line, '>'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_or_one_sided_changes_need_no_merge() {
        assert_eq!(merge_text("a\n", "a\n", "b\n"), Merged { text: "b\n".into(), conflicts: 0 });
        assert_eq!(merge_text("a\n", "b\n", "a\n"), Merged { text: "b\n".into(), conflicts: 0 });
        assert_eq!(merge_text("a\n", "c\n", "c\n"), Merged { text: "c\n".into(), conflicts: 0 });
    }

    #[test]
    fn combines_changes_to_different_lines() {
        let base = "one\n\ntwo\n\nthree\n";
        let mine = "one, edited\n\ntwo\n\nthree\n";
        let theirs = "one\n\ntwo\n\nthree, edited\nfour\n";
        assert_eq!(
            merge_text(base, mine, theirs),
            Merged { text: "one, edited\n\ntwo\n\nthree, edited\nfour\n".into(), conflicts: 0 }
        );
    }

    #[test]
    fn same_change_on_both_sides_is_not_a_conflict() {
        let merged = merge_text("a\nb\nc\n", "a\nB\nc\nmine\n", "a\nB\nc\n");
        assert_eq!(merged, Merged { text: "a\nB\nc\nmine\n".into(), conflicts: 0 });
    }

    #[test]
    fn overlapping_changes_are_marked_with_both_sides() {
        let merged = merge_text("intro\nprice: 10\nend\n", "intro\nprice: 12\nend\n", "intro\nprice: 15\nend\n");
        assert_eq!(merged.conflicts, 1);
        assert_eq!(
            merged.text,
            "intro\n<<<<<<< yours\nprice: 12\n=======\nprice: 15\n>>>>>>> theirs\nend\n"
        );
        assert!(has_conflict_markers(&merged.text));
    }

    #[test]
    fn counts_each_conflict_and_keeps_shared_lines_outside() {
        let base = "a\n\nb\n\nc\n\nd\n";
        let mine = "A1\n\nb\n\nc\n\nD1\n";
        let theirs = "A2\n\nb\n\nc\n\nD2\n";
        let merged = merge_text(base, mine, theirs);
        assert_eq!(merged.conflicts, 2);
        assert_eq!(
            merged.text,
            "<<<<<<< yours\nA1\n=======\nA2\n>>>>>>> theirs\n\nb\n\nc\n\n<<<<<<< yours\nD1\n=======\nD2\n>>>>>>> theirs\n"
        );
    }

    #[test]
    fn deletion_against_an_edit_keeps_the_edit_in_a_conflict() {
        let merged = merge_text("a\nb\nc\n", "a\nc\n", "a\nB\nc\n");
        assert_eq!(merged.text, "a\n<<<<<<< yours\n=======\nB\n>>>>>>> theirs\nc\n");
        assert_eq!(merged.conflicts, 1);
    }

    #[test]
    fn insertions_at_the_same_place_conflict() {
        let merged = merge_text("a\nz\n", "a\nmine\nz\n", "a\ntheirs\nz\n");
        assert_eq!(merged.text, "a\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> theirs\nz\n");
    }

    #[test]
    fn without_a_base_only_differing_lines_conflict() {
        let merged = merge_text("", "# Title\n\nmine\n\nend\n", "# Title\n\ntheirs\n\nend\n");
        assert_eq!(merged.conflicts, 1);
        assert_eq!(merged.text, "# Title\n\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> theirs\n\nend\n");
    }

    #[test]
    fn a_missing_final_newline_does_not_glue_markers_to_text() {
        let merged = merge_text("a\nb", "a\nmine", "a\ntheirs");
        assert_eq!(merged.text, "a\n<<<<<<< yours\nmine\n=======\ntheirs\n>>>>>>> theirs\n");
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let merged = merge_text("a\r\nb\r\n", "a\r\nmine\r\n", "a\r\ntheirs\r\n");
        assert_eq!(merged.text, "a\r\n<<<<<<< yours\r\nmine\r\n=======\r\ntheirs\r\n>>>>>>> theirs\r\n");
    }

    #[test]
    fn detects_leftover_markers_but_not_heading_underlines() {
        assert!(!has_conflict_markers("Title\n=======\n\ntext\n"));
        assert!(!has_conflict_markers("> quote\n>> nested\n<<<<<<<<< not a marker\n"));
        assert!(has_conflict_markers("text\n<<<<<<< HEAD\nx\n"));
        assert!(has_conflict_markers("x\n>>>>>>>\n"));
    }
}
