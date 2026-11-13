//! A saved arrangement of the picture, kept beside the model.
//!
//! The canvas is a projection of the model, so most of what a person does on it — moving a box,
//! resizing a container — has no SysML meaning. Writing those numbers into the model would put
//! presentation state into the thing that is supposed to be the source of truth, and every
//! downstream consumer would have to ignore it.
//!
//! They live here instead: a sidecar file, keyed by qualified name. The qualified name is the key
//! because the element id is a pre-order index that shifts when anything is inserted above it —
//! an id-keyed layout would move every box the first time someone added a declaration. A name
//! that no longer exists is dropped on load rather than carried as a ghost.
//!
//! # One arrangement per picture
//!
//! The general view and the interconnection view are different drawings of the same model, with
//! different boxes at different coordinates. They need separate arrangements: one shared map means
//! a box moved in the general view lands at that coordinate in the interconnection view, where it
//! is a different box drawn somewhere it was never put. The same holds for the two modes, since
//! compat emits a flat graph with no ports.
//!
//! So placements are stored per `<view>/<mode>`, and every read or write names the picture it is
//! for. A sidecar from the first release held a single map beside a view name; that file is read
//! into the picture it names rather than discarded.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where one box sits, in content coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    /// Width override. `None` keeps the width the layout computed, so a box that grows because
    /// its name grew still fits unless a person actually resized it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f32>,
}

/// The key a picture's arrangements are filed under.
pub fn picture(view: &str, mode: &str) -> String {
    format!("{view}/{mode}")
}

/// The on-disk shape.
///
/// `view`/`mode`/`nodes` are the first release's layout: one picture's arrangement, with the
/// picture named beside it. They are read and then folded into `views`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    views: BTreeMap<String, BTreeMap<String, Placement>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    view: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    mode: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    nodes: BTreeMap<String, Placement>,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    views: BTreeMap<String, BTreeMap<String, Placement>>,
    /// Which picture the caller is asking about. Request state rather than a fact about the file,
    /// so it is not persisted: the file holds every picture, the session looks at one.
    current: String,
}

impl Serialize for Layout {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Stored {
            views: self.views.clone(),
            ..Default::default()
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Layout {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Stored::deserialize(deserializer)?;
        let mut views = stored.views;
        if !stored.nodes.is_empty() {
            let view = if stored.view.is_empty() {
                "gv"
            } else {
                &stored.view
            };
            let mode = if stored.mode.is_empty() {
                "extended"
            } else {
                &stored.mode
            };
            views.insert(picture(view, mode), stored.nodes);
        }
        Ok(Self {
            views,
            current: String::new(),
        })
    }
}

impl Layout {
    /// A layout holding no placements, already pointed at one picture.
    pub fn empty(view: &str, mode: &str) -> Self {
        Self {
            views: BTreeMap::new(),
            current: picture(view, mode),
        }
    }

    /// Point subsequent `place`/`get`/`forget` calls at one picture.
    pub fn select(&mut self, view: &str, mode: &str) {
        self.current = picture(view, mode);
    }

    /// Load, or an empty layout when there is none yet.
    ///
    /// A file that does not parse is reported rather than silently replaced: the arrangement is
    /// work a person did, and overwriting it because of a stray character loses that work.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn place(&mut self, qualified: &str, placement: Placement) {
        self.views
            .entry(self.current.clone())
            .or_default()
            .insert(qualified.to_string(), placement);
    }

    pub fn get(&self, qualified: &str) -> Option<Placement> {
        self.views
            .get(&self.current)
            .and_then(|nodes| nodes.get(qualified))
            .copied()
    }

    pub fn forget(&mut self, qualified: &str) {
        if let Some(nodes) = self.views.get_mut(&self.current) {
            nodes.remove(qualified);
        }
    }

    /// Carry a saved position to a new qualified name, in every picture.
    ///
    /// A rename changes the name the arrangement is keyed by. Moving only the picture the person
    /// was looking at leaves the others holding a position for a name that no longer exists, and
    /// the box snaps back the next time they switch views.
    pub fn rename(&mut self, old: &str, new: &str) {
        for nodes in self.views.values_mut() {
            if let Some(place) = nodes.remove(old) {
                nodes.insert(new.to_string(), place);
            }
        }
    }

    /// How many placements are saved, across every picture.
    pub fn len(&self) -> usize {
        self.views.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The qualified names with a saved position in one picture.
    pub fn placed_in(&self, view: &str, mode: &str) -> Vec<String> {
        self.views
            .get(&picture(view, mode))
            .map(|nodes| nodes.keys().cloned().collect())
            .unwrap_or_default()
    }
}

/// The sidecar path for a model: `<model>.layout.json`.
pub fn path_for(model: &Path) -> PathBuf {
    let mut name = model
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string();
    name.push_str(".layout.json");
    model.with_file_name(name)
}

/// Drop placements whose element is gone, and report how many were dropped.
///
/// A renamed element keeps no entry: the rename changed its qualified name, and the placement
/// under the old name is a position for something that no longer exists. Reusing it for the new
/// name would be a guess about which box was meant.
pub fn prune(layout: &mut Layout, live: &[String]) -> usize {
    let mut dropped = 0;
    for nodes in layout.views.values_mut() {
        let before = nodes.len();
        nodes.retain(|name, _| live.contains(name));
        dropped += before - nodes.len();
    }
    layout.views.retain(|_, nodes| !nodes.is_empty());
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sysml-layout-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("m.sysml.layout.json")
    }

    #[test]
    fn a_layout_round_trips_through_its_sidecar() {
        let path = temp("roundtrip");
        let mut layout = Layout::empty("gv", "extended");
        layout.place(
            "P::Drone",
            Placement {
                x: 10.0,
                y: 20.0,
                w: None,
                h: None,
            },
        );
        layout.save(&path).unwrap();
        let mut back = Layout::load(&path).unwrap();
        back.select("gv", "extended");
        assert_eq!(back.get("P::Drone").unwrap().x, 10.0);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_missing_sidecar_is_not_an_error() {
        let path = std::env::temp_dir().join("sysml-does-not-exist.layout.json");
        std::fs::remove_file(&path).ok();
        assert!(Layout::load(&path).unwrap().is_empty());
    }

    #[test]
    fn pruning_drops_positions_for_elements_that_are_gone() {
        let mut layout = Layout::empty("gv", "extended");
        layout.place(
            "A",
            Placement {
                x: 0.0,
                y: 0.0,
                w: None,
                h: None,
            },
        );
        layout.place(
            "B",
            Placement {
                x: 1.0,
                y: 1.0,
                w: None,
                h: None,
            },
        );
        let dropped = prune(&mut layout, &["A".to_string()]);
        assert_eq!(dropped, 1);
        assert!(layout.get("B").is_none());
    }

    /// The bug this pins: one shared map meant a box dragged in the general view sat at that
    /// same coordinate in the interconnection view, where it is a different box.
    #[test]
    fn each_picture_keeps_its_own_arrangement() {
        let mut layout = Layout::empty("gv", "extended");
        layout.place(
            "P::Drone",
            Placement {
                x: 100.0,
                y: 200.0,
                w: None,
                h: None,
            },
        );
        layout.select("iv", "extended");
        assert!(
            layout.get("P::Drone").is_none(),
            "a general-view position is not an interconnection-view position"
        );
        layout.place(
            "P::Drone",
            Placement {
                x: 7.0,
                y: 8.0,
                w: None,
                h: None,
            },
        );
        layout.select("gv", "extended");
        assert_eq!(layout.get("P::Drone").unwrap().x, 100.0);
        layout.select("iv", "extended");
        assert_eq!(layout.get("P::Drone").unwrap().x, 7.0);
    }

    /// The same view in the other mode is a different drawing too: compat emits a flat graph
    /// with no ports, so a position saved against `extended` means nothing there.
    #[test]
    fn the_two_modes_are_separate_pictures() {
        let mut layout = Layout::empty("gv", "extended");
        layout.place(
            "P::Drone",
            Placement {
                x: 100.0,
                y: 200.0,
                w: None,
                h: None,
            },
        );
        layout.select("gv", "compat");
        assert!(layout.get("P::Drone").is_none());
    }

    #[test]
    fn a_rename_carries_the_position_in_every_picture() {
        let mut layout = Layout::empty("gv", "extended");
        layout.place(
            "P::Drone",
            Placement {
                x: 1.0,
                y: 1.0,
                w: None,
                h: None,
            },
        );
        layout.select("iv", "extended");
        layout.place(
            "P::Drone",
            Placement {
                x: 2.0,
                y: 2.0,
                w: None,
                h: None,
            },
        );
        layout.rename("P::Drone", "P::Quadcopter");
        layout.select("gv", "extended");
        assert_eq!(layout.get("P::Quadcopter").unwrap().x, 1.0);
        assert!(layout.get("P::Drone").is_none());
        layout.select("iv", "extended");
        assert_eq!(layout.get("P::Quadcopter").unwrap().x, 2.0);
    }

    /// A sidecar written by the first release named one picture and held one map. It still loads,
    /// into the picture it named.
    #[test]
    fn an_old_sidecar_is_read_into_the_picture_it_named() {
        let path = temp("legacy");
        std::fs::write(
            &path,
            r#"{
  "view": "iv",
  "mode": "compat",
  "nodes": { "P::Drone": { "x": 5.0, "y": 6.0 } }
}
"#,
        )
        .unwrap();
        let mut layout = Layout::load(&path).unwrap();
        layout.select("gv", "extended");
        assert!(layout.get("P::Drone").is_none(), "not in the general view");
        layout.select("iv", "compat");
        assert_eq!(layout.get("P::Drone").unwrap().x, 5.0);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
