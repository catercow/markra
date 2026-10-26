//! Tauri command for merging an open document with a change made outside the
//! editor. The logic is in `cowmark_core::merge`.

use cowmark_core::merge::{self, Merged};

#[tauri::command]
pub(crate) fn content_merge_text(base: String, mine: String, theirs: String) -> Merged {
    merge::merge_text(&base, &mine, &theirs)
}
