//! Recently opened files, one list per kind (`~/.local/share/nyx-office/recent-<kind>.json`).
//! Home shows them together.

use crate::app::{self, Kind};
use crate::{cmd, paths};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    /// Seconds since 1970 when it was last opened or saved.
    pub opened: u64,
}

const LIMIT: usize = 60;

fn file(kind: Kind) -> PathBuf {
    paths::recent_file(kind.id())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A kind's files, newest first, without files that have since been deleted.
pub fn list_of(kind: Kind) -> Vec<Recent> {
    let items: Vec<Recent> =
        std::fs::read_to_string(file(kind)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    items.into_iter().filter(|r| r.path.exists()).collect()
}

/// Every kind's files, newest first.
pub fn all() -> Vec<(Kind, Recent)> {
    let mut out: Vec<(Kind, Recent)> =
        Kind::ALL.into_iter().flat_map(|k| list_of(k).into_iter().map(move |r| (k, r))).collect();
    out.sort_by_key(|(_, r)| std::cmp::Reverse(r.opened));
    out
}

fn store(kind: Kind, items: &[Recent]) {
    if let Ok(text) = serde_json::to_string_pretty(items) {
        let _ = cmd::atomic_write(&file(kind), text);
    }
}

/// Note that a file was opened or saved.
pub fn record(path: &Path) {
    let kind = app::kind();
    let mut items = list_of(kind);
    items.retain(|r| r.path != path);
    items.insert(0, Recent { path: path.to_path_buf(), opened: now() });
    items.truncate(LIMIT);
    store(kind, &items);
}

pub fn forget(path: &Path) {
    for kind in Kind::ALL {
        let mut items = list_of(kind);
        let before = items.len();
        items.retain(|r| r.path != path);
        if items.len() != before {
            store(kind, &items);
        }
    }
}

/// "just now", "5 min ago", "3 hours ago", "yesterday", "12 days ago".
pub fn ago(then: u64) -> String {
    let d = now().saturating_sub(then);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..7200 => "1 hour ago".into(),
        7200..86400 => format!("{} hours ago", d / 3600),
        86400..172800 => "yesterday".into(),
        _ => format!("{} days ago", d / 86400),
    }
}

/// When, in a sentence: "today at 10:42", "yesterday at 18:20", "on Mar 4 at 09:15".
pub fn when_at(then: u64) -> String {
    let (Ok(t), Ok(now)) = (gtk::glib::DateTime::from_unix_local(then as i64), gtk::glib::DateTime::now_local()) else {
        return ago(then);
    };
    let day = |d: &gtk::glib::DateTime| (d.year(), d.day_of_year());
    let time = t.format("%H:%M").map(|s| s.to_string()).unwrap_or_default();
    if day(&t) == day(&now) {
        return format!("today at {time}");
    }
    if now.add_days(-1).is_ok_and(|y| day(&t) == day(&y)) {
        return format!("yesterday at {time}");
    }
    let f = if t.year() == now.year() { "%b %-d" } else { "%b %-d, %Y" };
    format!("on {} at {time}", t.format(f).map(|s| s.to_string()).unwrap_or_default())
}

/// When, for the recent files' list: "Today 10:42", "Yesterday", "Mar 4", "Mar 4, 2025".
pub fn when(then: u64) -> String {
    let (Ok(t), Ok(now)) = (gtk::glib::DateTime::from_unix_local(then as i64), gtk::glib::DateTime::now_local()) else {
        return ago(then);
    };
    let day = |d: &gtk::glib::DateTime| (d.year(), d.day_of_year());
    if day(&t) == day(&now) {
        return t.format("Today %H:%M").map(|s| s.to_string()).unwrap_or_else(|_| ago(then));
    }
    if let Ok(y) = now.add_days(-1)
        && day(&t) == day(&y)
    {
        return "Yesterday".into();
    }
    let f = if t.year() == now.year() { "%b %-d" } else { "%b %-d, %Y" };
    t.format(f).map(|s| s.to_string()).unwrap_or_else(|_| ago(then))
}
