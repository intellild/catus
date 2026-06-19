use crate::pane::any_view::PaneView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
  Horizontal,
  Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaneLeafId(pub u64);

#[derive(Clone)]
pub enum PaneNode {
  Leaf {
    id: PaneLeafId,
    view: PaneView,
  },
  Split {
    direction: SplitDirection,
    children: Vec<PaneNode>,
  },
}

impl PaneNode {
  pub fn new_leaf(id: PaneLeafId, view: PaneView) -> Self {
    PaneNode::Leaf { id, view }
  }

  pub fn leaf_count(&self) -> usize {
    match self {
      PaneNode::Leaf { .. } => 1,
      PaneNode::Split { children, .. } => children.iter().map(|c| c.leaf_count()).sum(),
    }
  }

  pub fn contains(&self, target: PaneLeafId) -> bool {
    match self {
      PaneNode::Leaf { id, .. } => *id == target,
      PaneNode::Split { children, .. } => children.iter().any(|c| c.contains(target)),
    }
  }

  /// Split the target leaf in the given direction, inserting new_leaf.
  /// Same direction: flatten by inserting new_leaf right after target sibling.
  /// Different direction: nest target into a new Split with new_leaf.
  pub fn split_at(
    &mut self,
    target: PaneLeafId,
    direction: SplitDirection,
    new_leaf: PaneNode,
  ) -> bool {
    match self {
      PaneNode::Leaf { id, view } if *id == target => {
        let old_view = view.clone();
        *self = PaneNode::Split {
          direction,
          children: vec![
            PaneNode::Leaf {
              id: *id,
              view: old_view,
            },
            new_leaf,
          ],
        };
        true
      }
      PaneNode::Leaf { .. } => false,
      PaneNode::Split {
        direction: my_dir,
        children,
      } => {
        if let Some(pos) = children.iter().position(|c| c.contains(target)) {
          if *my_dir == direction {
            children.insert(pos + 1, new_leaf);
          } else {
            children[pos].split_at(target, direction, new_leaf);
          }
          true
        } else {
          false
        }
      }
    }
  }

  /// Remove a leaf by id. Returns true if leaf was found and removed.
  pub fn remove_leaf(&mut self, target: PaneLeafId) -> bool {
    match self {
      PaneNode::Leaf { id, .. } => *id == target,
      PaneNode::Split { children, .. } => {
        let Some(pos) = children.iter().position(|c| c.contains(target)) else {
          return false;
        };
        if children[pos].is_leaf() {
          children.remove(pos);
          self.collapse_if_single();
          return true;
        }
        children[pos].remove_leaf(target);
        self.collapse_if_single();
        true
      }
    }
  }

  pub fn is_leaf(&self) -> bool {
    matches!(self, PaneNode::Leaf { .. })
  }

  /// 查找第一个叶子节点的视图（用于获取 tab 标题等）。
  pub fn first_leaf_view(&self) -> Option<&PaneView> {
    match self {
      PaneNode::Leaf { view, .. } => Some(view),
      PaneNode::Split { children, .. } => children.first().and_then(|c| c.first_leaf_view()),
    }
  }

  /// 按 ID 查找叶子节点的视图。
  pub fn find_view_by_id(&self, target: PaneLeafId) -> Option<&PaneView> {
    match self {
      PaneNode::Leaf { id, view } if *id == target => Some(view),
      PaneNode::Leaf { .. } => None,
      PaneNode::Split { children, .. } => children.iter().find_map(|c| c.find_view_by_id(target)),
    }
  }

  /// 按视图查找叶子节点的 ID。
  pub fn find_leaf_id_by_view(&self, target: &PaneView) -> Option<PaneLeafId> {
    match self {
      PaneNode::Leaf { id, view } if view == target => Some(*id),
      PaneNode::Leaf { .. } => None,
      PaneNode::Split { children, .. } => {
        children.iter().find_map(|c| c.find_leaf_id_by_view(target))
      }
    }
  }

  /// Collapse if only one child remains.
  fn collapse_if_single(&mut self) {
    if let PaneNode::Split { children, .. } = self
      && children.len() == 1
    {
      let child = children.remove(0);
      let _ = std::mem::replace(self, child);
    }
  }

  /// Find the next leaf id after the given one (depth-first in-order).
  pub fn next_leaf_after(&self, target: PaneLeafId) -> Option<PaneLeafId> {
    let mut found = false;
    self.next_leaf_inner(target, &mut found)
  }

  fn next_leaf_inner(&self, target: PaneLeafId, found: &mut bool) -> Option<PaneLeafId> {
    match self {
      PaneNode::Leaf { id, .. } => {
        if *found {
          return Some(*id);
        }
        if *id == target {
          *found = true;
        }
        None
      }
      PaneNode::Split { children, .. } => {
        for child in children {
          if let Some(id) = child.next_leaf_inner(target, found) {
            return Some(id);
          }
        }
        None
      }
    }
  }

  /// Find the previous leaf id before the given one (depth-first in-order).
  pub fn prev_leaf_before(&self, target: PaneLeafId) -> Option<PaneLeafId> {
    let mut prev = None;
    self.prev_leaf_inner(target, &mut prev);
    prev
  }

  fn prev_leaf_inner(&self, target: PaneLeafId, prev: &mut Option<PaneLeafId>) {
    match self {
      PaneNode::Leaf { id, .. } => {
        if *id == target {
          return;
        }
        *prev = Some(*id);
      }
      PaneNode::Split { children, .. } => {
        for child in children {
          if child.contains(target) {
            child.prev_leaf_inner(target, prev);
            return;
          }
          child.prev_leaf_inner(target, prev);
        }
      }
    }
  }
}
