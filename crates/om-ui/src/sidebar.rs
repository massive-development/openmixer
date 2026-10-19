//! Plain-data model of the browser sidebar: All tracks, Playlists (folders nest), Watch folders and
//! the Prep queue. Built from a [`LibraryTree`], so the tree is testable without the library. The SQLite-backed tree lives in `library/tree.rs`.

use std::path::Path;

use om_library::LibraryError;

/// Folders deeper than this stop the walk (a corrupt file must not recurse forever).
pub const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarKind {
    AllTracks,
    PlaylistGroup,
    Folder,
    Playlist,
    WatchGroup,
    WatchFolder,
    PrepQueue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarNode {
    pub kind: SidebarKind,
    pub label: String,
    /// Folder or playlist id.
    pub playlist_id: i64,
    /// Watch folder path.
    pub path: String,
    pub children: Vec<SidebarNode>,
}

impl SidebarNode {
    fn named(kind: SidebarKind, label: &str) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            playlist_id: 0,
            path: String::new(),
            children: Vec::new(),
        }
    }

    /// Groups (playlist, folder and watch headings) are not selectable.
    pub fn is_selectable(&self) -> bool {
        !matches!(
            self.kind,
            SidebarKind::PlaylistGroup | SidebarKind::Folder | SidebarKind::WatchGroup
        )
    }

    /// Query fragment that scopes the track list to this node (empty = everything). The prep queue
    /// has no query.
    pub fn scope_query(&self) -> String {
        match self.kind {
            SidebarKind::Playlist => format!("playlist:#{}", self.playlist_id),
            // Anchored prefix, not a substring. A quote in the path is doubled, which the query
            // grammar reads back as one quote.
            SidebarKind::WatchFolder => {
                format!("folder:\"{}\"", self.path.replace('"', "\"\""))
            }
            _ => String::new(),
        }
    }
}

/// One playlist or folder as the library reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    pub id: i64,
    pub name: String,
    pub is_folder: bool,
}

/// What the sidebar reads from the library.
pub trait LibraryTree {
    /// Children of `parent` (`None` = top level). An error fails the whole playlist group.
    fn playlist_children(&self, parent: Option<i64>) -> Result<Vec<PlaylistRow>, LibraryError>;
    /// Watch folder paths, in the library's order. An error leaves the watch group empty.
    fn watch_folders(&self) -> Result<Vec<String>, LibraryError>;
}

/// The sidebar nodes, and the first library read that failed while building them. A failed group
/// comes out empty, so `error` is what tells the user why it is empty.
#[derive(Debug, Default)]
pub struct Sidebar {
    pub nodes: Vec<SidebarNode>,
    pub error: Option<LibraryError>,
}

fn add_playlists(
    lib: &dyn LibraryTree,
    parent: Option<i64>,
    out: &mut Vec<SidebarNode>,
    depth: usize,
    error: &mut Option<LibraryError>,
) -> bool {
    let items = match lib.playlist_children(parent) {
        Ok(items) => items,
        Err(e) => {
            error.get_or_insert(e);
            return false;
        }
    };
    if depth > MAX_DEPTH {
        return false;
    }
    for item in items {
        let kind = if item.is_folder {
            SidebarKind::Folder
        } else {
            SidebarKind::Playlist
        };
        let mut node = SidebarNode::named(kind, &item.name);
        node.playlist_id = item.id;
        if item.is_folder
            && !add_playlists(lib, Some(item.id), &mut node.children, depth + 1, error)
        {
            return false;
        }
        out.push(node);
    }
    true
}

/// Leaf name, or "leaf (parent)" when another watch folder shares the leaf, so rows stay distinct.
fn watch_label(path: &str, all: &[String]) -> String {
    let p = Path::new(path);
    let leaf = p
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
    let same = all
        .iter()
        .filter(|other| {
            Path::new(other.as_str())
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                == Some(leaf.clone())
        })
        .count();
    if same > 1 {
        let parent = p
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        format!("{leaf} ({parent})")
    } else {
        leaf
    }
}

/// Top-level nodes in display order. A failed playlist read yields an empty group, never a partial
/// tree; a failed watch-folder read yields an empty watch group. Either failure is in the result.
pub fn build_sidebar(lib: &dyn LibraryTree) -> Sidebar {
    let mut error = None;
    let mut top = vec![SidebarNode::named(SidebarKind::AllTracks, "All tracks")];

    let mut playlists = SidebarNode::named(SidebarKind::PlaylistGroup, "Playlists");
    if !add_playlists(lib, None, &mut playlists.children, 0, &mut error) {
        playlists.children.clear();
    }
    top.push(playlists);

    let mut watch = SidebarNode::named(SidebarKind::WatchGroup, "Watch folders");
    let paths = match lib.watch_folders() {
        Ok(paths) => paths,
        Err(e) => {
            error.get_or_insert(e);
            Vec::new()
        }
    };
    for path in &paths {
        let mut node = SidebarNode::named(SidebarKind::WatchFolder, &watch_label(path, &paths));
        node.path = path.clone();
        watch.children.push(node);
    }
    top.push(watch);

    top.push(SidebarNode::named(SidebarKind::PrepQueue, "Prep queue"));
    Sidebar { nodes: top, error }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree {
        playlists: Vec<(Option<i64>, PlaylistRow)>,
        watch: Vec<String>,
        broken_parent: Option<Option<i64>>,
        broken_watch: bool,
    }

    impl LibraryTree for Tree {
        fn playlist_children(&self, parent: Option<i64>) -> Result<Vec<PlaylistRow>, LibraryError> {
            if self.broken_parent == Some(parent) {
                return Err(LibraryError::Setup("read failed".into()));
            }
            Ok(self
                .playlists
                .iter()
                .filter(|(p, _)| *p == parent)
                .map(|(_, r)| r.clone())
                .collect())
        }

        fn watch_folders(&self) -> Result<Vec<String>, LibraryError> {
            if self.broken_watch {
                return Err(LibraryError::Setup("watch folders unreadable".into()));
            }
            Ok(self.watch.clone())
        }
    }

    fn row(id: i64, name: &str, is_folder: bool) -> PlaylistRow {
        PlaylistRow {
            id,
            name: name.into(),
            is_folder,
        }
    }

    #[test]
    fn nested_playlists_and_watch_folders_keep_their_order() {
        let tree = Tree {
            playlists: vec![
                (None, row(1, "Warmup", false)),
                (None, row(2, "Sets", true)),
                (Some(2), row(3, "Friday", false)),
            ],
            watch: vec!["/music/a".into()],
            broken_parent: None,
            broken_watch: false,
        };
        let top = build_sidebar(&tree).nodes;
        let labels: Vec<&str> = top.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            labels,
            ["All tracks", "Playlists", "Watch folders", "Prep queue"]
        );
        let sets = &top[1].children[1];
        assert_eq!(sets.kind, SidebarKind::Folder);
        assert!(!sets.is_selectable());
        assert_eq!(sets.children[0].playlist_id, 3);
        assert_eq!(sets.children[0].scope_query(), "playlist:#3");
        assert!(top[1].children[0].is_selectable());
    }

    #[test]
    fn a_failed_playlist_read_yields_an_empty_group() {
        let tree = Tree {
            playlists: vec![(None, row(1, "Warmup", false))],
            watch: Vec::new(),
            broken_parent: Some(None),
            broken_watch: false,
        };
        let top = build_sidebar(&tree).nodes;
        assert!(top[1].children.is_empty(), "no partial tree");
    }

    #[test]
    fn a_failed_watch_folder_read_is_reported_not_silently_empty() {
        let tree = Tree {
            playlists: vec![(None, row(1, "Warmup", false))],
            watch: vec!["/music/a".into()],
            broken_parent: None,
            broken_watch: true,
        };
        let sidebar = build_sidebar(&tree);
        assert!(
            sidebar.nodes[2].children.is_empty(),
            "no watch folders shown"
        );
        assert_eq!(sidebar.nodes[1].children.len(), 1, "playlists still listed");
        let error = sidebar.error.expect("the failure is reported");
        assert_eq!(error.to_string(), "watch folders unreadable");
    }

    #[test]
    fn a_healthy_tree_reports_no_error() {
        let tree = Tree {
            playlists: Vec::new(),
            watch: vec!["/music/a".into()],
            broken_parent: None,
            broken_watch: false,
        };
        assert!(build_sidebar(&tree).error.is_none());
    }

    #[test]
    fn a_failed_nested_read_clears_the_whole_group() {
        let tree = Tree {
            playlists: vec![
                (None, row(2, "Sets", true)),
                (Some(2), row(3, "Friday", false)),
            ],
            watch: Vec::new(),
            broken_parent: Some(Some(2)),
            broken_watch: false,
        };
        assert!(build_sidebar(&tree).nodes[1].children.is_empty());
    }

    #[test]
    fn deep_folders_stop_instead_of_recursing_forever() {
        let mut playlists = Vec::new();
        let mut parent = None;
        for id in 1..=(MAX_DEPTH as i64 + 4) {
            playlists.push((parent, row(id, "f", true)));
            parent = Some(id);
        }
        let tree = Tree {
            playlists,
            watch: Vec::new(),
            broken_parent: None,
            broken_watch: false,
        };
        assert!(build_sidebar(&tree).nodes[1].children.is_empty());
    }

    #[test]
    fn watch_folders_with_the_same_leaf_name_show_their_parent() {
        let tree = Tree {
            playlists: Vec::new(),
            watch: vec![
                "/music/a/House".into(),
                "/music/b/House".into(),
                "/music/c/Techno".into(),
            ],
            broken_parent: None,
            broken_watch: false,
        };
        let top = build_sidebar(&tree).nodes;
        let labels: Vec<&str> = top[2].children.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["House (/music/a)", "House (/music/b)", "Techno"]);
        assert_eq!(
            top[2].children[2].scope_query(),
            "folder:\"/music/c/Techno\""
        );
    }

    #[test]
    fn groups_and_all_tracks_have_no_scope() {
        let tree = Tree {
            playlists: Vec::new(),
            watch: Vec::new(),
            broken_parent: None,
            broken_watch: false,
        };
        let top = build_sidebar(&tree).nodes;
        assert!(top[0].scope_query().is_empty());
        assert!(top[3].scope_query().is_empty(), "prep queue has no query");
    }

    #[test]
    fn watch_folder_scope_keeps_a_quote_in_the_path() {
        let node = SidebarNode {
            kind: SidebarKind::WatchFolder,
            label: "say".to_owned(),
            playlist_id: 0,
            path: r#"/m/say "hi""#.to_owned(),
            children: Vec::new(),
        };
        // Escaped as a doubled quote, not stripped: the path must still match its tracks.
        assert_eq!(node.scope_query(), r#"folder:"/m/say ""hi""""#);
    }
}
