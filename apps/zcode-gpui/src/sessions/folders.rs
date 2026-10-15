//! Project folder open/closed state in the sidebar. A folder opens when it is
//! clicked or becomes the active project, and stays open when another project
//! becomes active, until the user closes it (desktop parity:
//! workspace-grouped-tasks/group-item.tsx handleHeaderClick only toggles).

use std::collections::HashSet;

#[derive(Default)]
pub(crate) struct Folders {
    /// Closed by the user; wins over `opened`.
    pub(crate) collapsed: HashSet<String>,
    pub(crate) opened: HashSet<String>,
}

impl Folders {
    /// Whether `key` renders open; the active project opens unless closed.
    pub(crate) fn is_open(&mut self, key: &str, active: bool) -> bool {
        if self.collapsed.contains(key) {
            return false;
        }
        if active {
            self.opened.insert(key.to_string());
        }
        self.opened.contains(key)
    }

    /// Header click on a folder that rendered `was_open`.
    pub(crate) fn toggle(&mut self, key: &str, was_open: bool) {
        if was_open {
            self.collapsed.insert(key.to_string());
            self.opened.remove(key);
        } else {
            self.collapsed.remove(key);
            self.opened.insert(key.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Folders;

    #[test]
    fn folders_stay_open_when_another_project_becomes_active() {
        let mut f = Folders::default();
        assert!(f.is_open("a", true), "active project opens");
        assert!(!f.is_open("b", false));
        f.toggle("b", false);
        assert!(f.is_open("b", true));
        assert!(f.is_open("a", false), "a stays open after b became active");
        f.toggle("a", true);
        assert!(!f.is_open("a", false));
        assert!(
            !f.is_open("a", true),
            "closed by the user, even when active"
        );
        f.toggle("a", false);
        assert!(f.is_open("a", false));
    }
}
