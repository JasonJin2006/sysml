//! Local drawing measurements; symbol semantics follow SysML v2 graphical notation.

/// Corner radius of a usage box. A definition box has no radius: the notation draws definitions
/// square and usages rounded, and that is the first thing a reader uses to tell them apart.
pub const RADIUS_USAGE: f32 = 6.0;

/// Stroke width of every outline and of the connectors.
pub const STROKE: f32 = 0.5;

/// Text size of everything inside a box: stereotype, name, compartment rows.
pub const TEXT: f32 = 10.0;

/// Text size of a compartment's heading (`attributes`), one step below the rows it labels.
pub const TEXT_SMALL: f32 = 9.0;

/// Ink of the stereotype line. Blue rather than black, and italic, which is how the notation
/// separates the kind of an element from its name.
pub const INK_STEREOTYPE: &str = "#163366";

/// Ink of a name and of the body text.
pub const INK: &str = "#1A1A1A";

/// Ink of a compartment heading and of secondary text.
pub const INK_MUTED: &str = "#5A6470";

/// The font family a box's text is set in.
pub const FONT: &str = "Roboto, Helvetica, Arial, sans-serif";

/// Local arrowhead length, not a dimension mandated by the language specification.
pub const HEAD_LENGTH: f32 = 8.0;

/// Local half-width for the rendered arrowhead, not a normative SysML dimension.
pub const HEAD_HALF_WIDTH: f32 = 4.0;

/// The five decorations a connector can end in, as the notation draws them.
///
/// Each is a closed path in a local frame where the tip sits at `(HEAD_LENGTH, 0)` and the shape
/// opens back along `-x`. The coordinates are the reference's own, so a symbol cannot be
/// re-proportioned by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    /// `-->` an open arrow: two strokes, no fill. A dependency, an `import`, a `satisfy`.
    Open,
    /// `--▸` a filled triangle. An allocation, and the notation's plain directed relationship.
    Filled,
    /// `--▷` a hollow triangle. Specialization: `:>` and typing.
    Hollow,
    /// `--◆` a filled diamond. Composition and ownership.
    FilledDiamond,
    /// `--◇` a hollow diamond. Shared aggregation.
    HollowDiamond,
}

impl Head {
    /// The head's outline as a list of subpaths, in a frame where the tip is the origin and the
    /// shape opens towards `-x`.
    ///
    /// Two subpaths for the open arrow (two strokes), one closed subpath for each filled or hollow
    /// shape. Returning points rather than a path string is what lets a caller rotate the frame
    /// without parsing its own output back.
    pub fn outline(self) -> (Vec<Vec<(f32, f32)>>, bool) {
        let half = HEAD_HALF_WIDTH;
        let len = HEAD_LENGTH;
        match self {
            Head::Open => (
                vec![
                    vec![(0.0, 0.0), (len, half)],
                    vec![(0.0, 0.0), (len, -half)],
                ],
                false,
            ),
            Head::Filled => (vec![vec![(0.0, 0.0), (len, half), (len, -half)]], true),
            Head::Hollow => (vec![vec![(0.0, 0.0), (len, half), (len, -half)]], false),
            Head::FilledDiamond => (
                vec![vec![
                    (0.0, 0.0),
                    (len, half),
                    (len * 2.0, 0.0),
                    (len, -half),
                ]],
                true,
            ),
            Head::HollowDiamond => (
                vec![vec![
                    (0.0, 0.0),
                    (len, half),
                    (len * 2.0, 0.0),
                    (len, -half),
                ]],
                false,
            ),
        }
    }

    /// How far back from the tip the connector's line has to stop.
    ///
    /// A filled head hides the line under it; a hollow one does not, so the line is stopped short
    /// or it shows through the shape and the symbol reads as struck out.
    pub fn reach(self) -> f32 {
        match self {
            Head::FilledDiamond | Head::HollowDiamond => HEAD_LENGTH * 2.0,
            _ => HEAD_LENGTH,
        }
    }
}

/// The stick-figure form permitted for an actor; stakeholder uses a labelled rectangle.
pub fn actor_figure(cx: f32, top: f32, ink: &str, stroke: f32) -> String {
    let head_cy = top + 10.0;
    let head_r = 7.0;
    let body_top = head_cy + head_r;
    let body_bottom = top + 62.0;
    let arm_y = top + 35.0;
    let leg_y = top + 96.0;
    format!(
        "<circle cx=\"{cx:.1}\" cy=\"{head_cy:.1}\" r=\"{head_r:.1}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"{stroke}\"/>\
<path d=\"M{cx:.1},{body_top:.1} L{cx:.1},{body_bottom:.1} M{left:.1},{arm_y:.1} L{right:.1},{arm_y:.1} M{cx:.1},{body_bottom:.1} L{leg_l:.1},{leg_y:.1} M{cx:.1},{body_bottom:.1} L{leg_r:.1},{leg_y:.1}\" \
fill=\"none\" stroke=\"{ink}\" stroke-width=\"{stroke}\"/>",
        left = cx - 19.0,
        right = cx + 19.0,
        leg_l = cx - 17.0,
        leg_r = cx + 17.0,
    )
}

/// The box a stick figure needs: wide enough for the arms, tall enough for the legs and a name.
pub const ACTOR_W: f32 = 44.0;
/// Height of the figure itself, above the name written under it.
pub const ACTOR_H: f32 = 96.0;

/// Whether an element of this kind is drawn as a person rather than as a box.
pub fn is_actor(stereotype: &str) -> bool {
    let kind = stereotype
        .trim()
        .trim_start_matches('«')
        .trim_end_matches('»');
    kind == "actor"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_head_fits_the_same_eight_unit_box() {
        for head in [
            Head::Open,
            Head::Filled,
            Head::Hollow,
            Head::FilledDiamond,
            Head::HollowDiamond,
        ] {
            let (subpaths, _) = head.outline();
            assert!(!subpaths.is_empty(), "{head:?} draws something");
            assert!(
                subpaths.iter().all(|p| p.len() >= 2),
                "{head:?} has a line to draw"
            );
            assert!(
                head.reach() >= HEAD_LENGTH,
                "{head:?} is at least a head long"
            );
        }
    }

    #[test]
    fn only_the_filled_symbols_are_filled() {
        assert!(Head::Filled.outline().1);
        assert!(Head::FilledDiamond.outline().1);
        assert!(
            !Head::Open.outline().1,
            "an open arrow is strokes, not a shape"
        );
        assert!(!Head::Hollow.outline().1);
        assert!(!Head::HollowDiamond.outline().1);
    }

    #[test]
    fn an_open_arrow_is_two_strokes_from_one_tip() {
        let (subpaths, _) = Head::Open.outline();
        assert_eq!(subpaths.len(), 2);
        assert_eq!(subpaths[0][0], (0.0, 0.0));
        assert_eq!(subpaths[1][0], (0.0, 0.0));
    }

    #[test]
    fn a_diamond_is_twice_a_head_long() {
        assert_eq!(Head::FilledDiamond.reach(), HEAD_LENGTH * 2.0);
        assert_eq!(Head::Filled.reach(), HEAD_LENGTH);
    }

    #[test]
    fn an_actor_is_a_person_and_a_part_is_not() {
        assert!(is_actor("«actor»"));
        assert!(!is_actor("«stakeholder»"));
        assert!(!is_actor("«part»"));
        assert!(!is_actor(""));
    }

    #[test]
    fn the_figure_is_a_head_and_four_limbs() {
        let figure = actor_figure(50.0, 10.0, INK, STROKE);
        assert!(figure.contains("<circle"), "a head");
        assert!(
            figure.contains("M50.0,27.0 L50.0,72.0"),
            "torso meets the head at cy + radius"
        );
        // Body, two arms and two legs: four strokes in one path.
        assert_eq!(figure.matches('M').count(), 4);
        assert!(figure.contains("stroke-width=\"0.5\""));
    }

    #[test]
    fn local_drawing_measurements_remain_consistent() {
        assert_eq!(RADIUS_USAGE, 6.0);
        assert_eq!(STROKE, 0.5);
        assert_eq!(TEXT, 10.0);
        assert_eq!(HEAD_LENGTH, 8.0);
        assert_eq!(HEAD_HALF_WIDTH, 4.0);
    }
}
