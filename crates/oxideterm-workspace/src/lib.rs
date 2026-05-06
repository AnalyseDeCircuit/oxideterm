use std::fmt;

pub const MAX_PANES_PER_TAB: usize = 4;
pub const MIN_PANE_FRACTION: f32 = 10.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TerminalSessionId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TabId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PaneId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TabKind {
    LocalTerminal,
    SshTerminal,
    SessionManager,
    Settings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TabTitleSource {
    Static,
    I18nKey(&'static str),
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: TabId,
    pub kind: TabKind,
    pub title: String,
    pub title_source: TabTitleSource,
    pub root_pane: Option<PaneNode>,
    pub active_pane_id: Option<PaneId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaneNode {
    Leaf {
        pane_id: PaneId,
        session_id: TerminalSessionId,
    },
    Group {
        id: PaneId,
        direction: SplitDirection,
        children: Vec<PaneNode>,
        sizes: Vec<f32>,
    },
}

impl PaneNode {
    pub fn leaf(pane_id: PaneId, session_id: TerminalSessionId) -> Self {
        Self::Leaf {
            pane_id,
            session_id,
        }
    }

    pub fn pane_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Group { children, .. } => children.iter().map(Self::pane_count).sum(),
        }
    }

    pub fn first_pane_id(&self) -> PaneId {
        match self {
            Self::Leaf { pane_id, .. } => *pane_id,
            Self::Group { children, .. } => children[0].first_pane_id(),
        }
    }

    pub fn contains_pane(&self, target: PaneId) -> bool {
        match self {
            Self::Leaf { pane_id, .. } => *pane_id == target,
            Self::Group { children, .. } => {
                children.iter().any(|child| child.contains_pane(target))
            }
        }
    }

    pub fn collect_pane_ids(&self, panes: &mut Vec<PaneId>) {
        match self {
            Self::Leaf { pane_id, .. } => panes.push(*pane_id),
            Self::Group { children, .. } => {
                for child in children {
                    child.collect_pane_ids(panes);
                }
            }
        }
    }

    pub fn split_active(
        &mut self,
        active_pane_id: PaneId,
        group_id: PaneId,
        direction: SplitDirection,
        new_pane_id: PaneId,
        new_session_id: TerminalSessionId,
    ) -> bool {
        match self {
            Self::Leaf {
                pane_id,
                session_id,
            } if *pane_id == active_pane_id => {
                let old = Self::Leaf {
                    pane_id: *pane_id,
                    session_id: *session_id,
                };
                *self = Self::Group {
                    id: group_id,
                    direction,
                    children: vec![old, Self::leaf(new_pane_id, new_session_id)],
                    sizes: vec![50.0, 50.0],
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Group { children, .. } => children.iter_mut().any(|child| {
                child.split_active(
                    active_pane_id,
                    group_id,
                    direction,
                    new_pane_id,
                    new_session_id,
                )
            }),
        }
    }

    pub fn close_pane(&mut self, target: PaneId) -> Option<PaneId> {
        match self {
            Self::Leaf { .. } => None,
            Self::Group {
                children, sizes, ..
            } => {
                let mut removed = false;
                let mut index = 0;
                while index < children.len() {
                    if matches!(&children[index], Self::Leaf { pane_id, .. } if *pane_id == target)
                    {
                        children.remove(index);
                        if index < sizes.len() {
                            sizes.remove(index);
                        }
                        removed = true;
                        break;
                    }
                    if children[index].contains_pane(target) {
                        let fallback = children[index].close_pane(target);
                        if let Some(replacement) = children[index].single_child_replacement() {
                            children[index] = replacement;
                        }
                        removed = fallback.is_some();
                        break;
                    }
                    index += 1;
                }

                if !removed {
                    return None;
                }

                normalize_sizes(sizes, children.len());
                children.first().map(Self::first_pane_id)
            }
        }
    }

    pub fn single_child_replacement(&mut self) -> Option<PaneNode> {
        match self {
            Self::Group { children, .. } if children.len() == 1 => Some(children.remove(0)),
            _ => None,
        }
    }

    pub fn update_group_sizes(&mut self, group_id: PaneId, next_sizes: &[f32]) -> bool {
        match self {
            Self::Leaf { .. } => false,
            Self::Group {
                id,
                children,
                sizes,
                ..
            } if *id == group_id && next_sizes.len() == children.len() => {
                *sizes = balanced_sizes(next_sizes, children.len());
                true
            }
            Self::Group { children, .. } => children
                .iter_mut()
                .any(|child| child.update_group_sizes(group_id, next_sizes)),
        }
    }
}

pub fn adjusted_split_sizes(
    start_sizes: &[f32],
    handle_index: usize,
    delta_fraction: f32,
) -> Vec<f32> {
    if handle_index + 1 >= start_sizes.len() {
        return start_sizes.to_vec();
    }

    let mut sizes = start_sizes.to_vec();
    let left = start_sizes[handle_index] + delta_fraction;
    let total = start_sizes[handle_index] + start_sizes[handle_index + 1];
    let left = left.clamp(MIN_PANE_FRACTION, total - MIN_PANE_FRACTION);
    sizes[handle_index] = left;
    sizes[handle_index + 1] = total - left;
    sizes
}

pub fn balanced_sizes(sizes: &[f32], count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    if sizes.len() != count {
        return equal_sizes(count);
    }
    let total: f32 = sizes.iter().copied().sum();
    if total <= f32::EPSILON {
        return equal_sizes(count);
    }
    sizes.iter().map(|size| (size / total) * 100.0).collect()
}

pub fn equal_sizes(count: usize) -> Vec<f32> {
    vec![100.0 / count as f32; count]
}

fn normalize_sizes(sizes: &mut Vec<f32>, count: usize) {
    *sizes = balanced_sizes(sizes, count);
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tab-{}", self.0)
    }
}

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pane-{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (PaneId, PaneId, PaneId, TerminalSessionId, TerminalSessionId) {
        (
            PaneId(1),
            PaneId(2),
            PaneId(3),
            TerminalSessionId(1),
            TerminalSessionId(2),
        )
    }

    #[test]
    fn split_active_leaf_creates_group_and_focusable_leaf() {
        let (pane_a, pane_b, group, session_a, session_b) = ids();
        let mut node = PaneNode::leaf(pane_a, session_a);

        assert!(node.split_active(pane_a, group, SplitDirection::Horizontal, pane_b, session_b));
        assert_eq!(node.pane_count(), 2);
        assert!(node.contains_pane(pane_a));
        assert!(node.contains_pane(pane_b));
    }

    #[test]
    fn close_pane_collapses_group_to_remaining_leaf() {
        let (pane_a, pane_b, group, session_a, session_b) = ids();
        let mut node = PaneNode::Group {
            id: group,
            direction: SplitDirection::Horizontal,
            children: vec![
                PaneNode::leaf(pane_a, session_a),
                PaneNode::leaf(pane_b, session_b),
            ],
            sizes: vec![50.0, 50.0],
        };

        assert_eq!(node.close_pane(pane_b), Some(pane_a));
        if let Some(replacement) = node.single_child_replacement() {
            node = replacement;
        }
        assert_eq!(node, PaneNode::leaf(pane_a, session_a));
    }

    #[test]
    fn adjusted_split_sizes_clamps_adjacent_panes() {
        assert_eq!(
            adjusted_split_sizes(&[50.0, 50.0], 0, 80.0),
            vec![90.0, 10.0]
        );
        assert_eq!(
            adjusted_split_sizes(&[20.0, 80.0], 0, -50.0),
            vec![10.0, 90.0]
        );
    }
}
