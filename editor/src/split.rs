use ratatui::layout::Rect;

use crate::buffer::{Cursor, Selection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal, // Split top/bottom (rows)
    Vertical,   // Split left/right (columns)
}

#[derive(Debug, Clone)]
pub struct PaneState {
    pub id: usize,
    pub buffer_index: usize,
    pub cursor: Cursor,
    pub secondary_cursors: Vec<Cursor>,
    pub selection: Option<Selection>,
    pub scroll_top: usize,
    pub scroll_left: usize,
}

impl PaneState {
    pub fn new(id: usize, buffer_index: usize) -> Self {
        Self {
            id,
            buffer_index,
            cursor: Cursor::default(),
            secondary_cursors: Vec::new(),
            selection: None,
            scroll_top: 0,
            scroll_left: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SplitNode {
    Leaf(PaneState),
    Container {
        direction: SplitDirection,
        children: Vec<SplitNode>,
        ratios: Vec<u16>, // Proportions, summing to e.g. 100
    },
}

#[derive(Debug, Clone)]
pub struct SplitTree {
    pub root: SplitNode,
    pub active_pane_id: usize,
    next_pane_id: usize,
}

impl SplitTree {
    pub fn new(initial_buffer_idx: usize) -> Self {
        let root = SplitNode::Leaf(PaneState::new(0, initial_buffer_idx));
        Self {
            root,
            active_pane_id: 0,
            next_pane_id: 1,
        }
    }

    pub fn active_pane(&self) -> Option<&PaneState> {
        self.find_pane_by_id(self.active_pane_id)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut PaneState> {
        let target_id = self.active_pane_id;
        self.find_pane_by_id_mut(target_id)
    }

    pub fn find_pane_by_id(&self, id: usize) -> Option<&PaneState> {
        Self::find_in_node(&self.root, id)
    }

    fn find_in_node<'a>(node: &'a SplitNode, id: usize) -> Option<&'a PaneState> {
        match node {
            SplitNode::Leaf(pane) => {
                if pane.id == id {
                    Some(pane)
                } else {
                    None
                }
            }
            SplitNode::Container { children, .. } => {
                for child in children {
                    if let Some(p) = Self::find_in_node(child, id) {
                        return Some(p);
                    }
                }
                None
            }
        }
    }

    pub fn find_pane_by_id_mut(&mut self, id: usize) -> Option<&mut PaneState> {
        Self::find_in_node_mut(&mut self.root, id)
    }

    fn find_in_node_mut<'a>(node: &'a mut SplitNode, id: usize) -> Option<&'a mut PaneState> {
        match node {
            SplitNode::Leaf(pane) => {
                if pane.id == id {
                    Some(pane)
                } else {
                    None
                }
            }
            SplitNode::Container { children, .. } => {
                for child in children {
                    if let Some(p) = Self::find_in_node_mut(child, id) {
                        return Some(p);
                    }
                }
                None
            }
        }
    }

    /// Count total number of leaf panes.
    pub fn pane_count(&self) -> usize {
        Self::count_panes(&self.root)
    }

    fn count_panes(node: &SplitNode) -> usize {
        match node {
            SplitNode::Leaf(_) => 1,
            SplitNode::Container { children, .. } => {
                children.iter().map(Self::count_panes).sum()
            }
        }
    }

    /// List all leaf panes in order.
    pub fn all_panes(&self) -> Vec<&PaneState> {
        let mut list = Vec::new();
        Self::collect_panes(&self.root, &mut list);
        list
    }

    fn collect_panes<'a>(node: &'a SplitNode, list: &mut Vec<&'a PaneState>) {
        match node {
            SplitNode::Leaf(pane) => list.push(pane),
            SplitNode::Container { children, .. } => {
                for child in children {
                    Self::collect_panes(child, list);
                }
            }
        }
    }

    /// Split the active pane in the specified direction.
    /// Clones the buffer index and state of the current active pane into the new pane.
    pub fn split_active(&mut self, direction: SplitDirection, new_buffer_idx: Option<usize>) -> usize {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        let cur_pane = self.active_pane().cloned();
        let buffer_idx = new_buffer_idx.unwrap_or_else(|| {
            cur_pane.as_ref().map(|p| p.buffer_index).unwrap_or(0)
        });

        let mut new_pane = PaneState::new(new_id, buffer_idx);
        if let Some(ref cur) = cur_pane {
            new_pane.cursor = cur.cursor;
            new_pane.scroll_top = cur.scroll_top;
            new_pane.scroll_left = cur.scroll_left;
        }

        let target_id = self.active_pane_id;
        Self::split_node(&mut self.root, target_id, direction, new_pane);
        self.active_pane_id = new_id;
        new_id
    }

    fn split_node(
        node: &mut SplitNode,
        target_id: usize,
        direction: SplitDirection,
        new_pane: PaneState,
    ) -> bool {
        match node {
            SplitNode::Leaf(pane) => {
                if pane.id == target_id {
                    let existing_pane = pane.clone();
                    *node = SplitNode::Container {
                        direction,
                        children: vec![
                            SplitNode::Leaf(existing_pane),
                            SplitNode::Leaf(new_pane),
                        ],
                        ratios: vec![50, 50],
                    };
                    return true;
                }
                false
            }
            SplitNode::Container { children, ratios, direction: cont_dir } => {
                for (idx, child) in children.iter_mut().enumerate() {
                    if let SplitNode::Leaf(p) = child {
                        if p.id == target_id {
                            if *cont_dir == direction {
                                // Add directly to container
                                children.insert(idx + 1, SplitNode::Leaf(new_pane));
                                // Rebalance ratios
                                let n = children.len();
                                *ratios = vec![100 / (n as u16); n];
                                return true;
                            } else {
                                // Sub-split with new direction
                                let existing = p.clone();
                                *child = SplitNode::Container {
                                    direction,
                                    children: vec![
                                        SplitNode::Leaf(existing),
                                        SplitNode::Leaf(new_pane),
                                    ],
                                    ratios: vec![50, 50],
                                };
                                return true;
                            }
                        }
                    } else if Self::split_node(child, target_id, direction, new_pane.clone()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Close the active pane. If it's the last pane, it cannot be closed.
    pub fn close_active_pane(&mut self) -> bool {
        if self.pane_count() <= 1 {
            return false;
        }
        let target_id = self.active_pane_id;
        let closed = Self::remove_node(&mut self.root, target_id);
        if closed {
            let all = self.all_panes();
            if let Some(first) = all.first() {
                self.active_pane_id = first.id;
            }
        }
        closed
    }

    fn remove_node(node: &mut SplitNode, target_id: usize) -> bool {
        match node {
            SplitNode::Leaf(_) => false,
            SplitNode::Container { children, ratios, .. } => {
                let mut remove_idx = None;
                for (i, child) in children.iter().enumerate() {
                    if let SplitNode::Leaf(p) = child {
                        if p.id == target_id {
                            remove_idx = Some(i);
                            break;
                        }
                    }
                }

                if let Some(idx) = remove_idx {
                    children.remove(idx);
                    if children.len() == 1 {
                        // Collapse container into its remaining single child
                        *node = children.remove(0);
                    } else {
                        let n = children.len();
                        *ratios = vec![100 / (n as u16); n];
                    }
                    return true;
                }

                for child in children.iter_mut() {
                    if Self::remove_node(child, target_id) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Close all other panes except the active one (Zoom / Only).
    pub fn only_active_pane(&mut self) {
        if let Some(pane) = self.active_pane().cloned() {
            self.root = SplitNode::Leaf(pane);
        }
    }

    /// Navigate to next or previous pane.
    pub fn cycle_pane(&mut self, forward: bool) {
        let panes = self.all_panes();
        if panes.is_empty() {
            return;
        }
        let cur_idx = panes
            .iter()
            .position(|p| p.id == self.active_pane_id)
            .unwrap_or(0);
        let next_idx = if forward {
            (cur_idx + 1) % panes.len()
        } else if cur_idx == 0 {
            panes.len() - 1
        } else {
            cur_idx - 1
        };
        self.active_pane_id = panes[next_idx].id;
    }

    /// Compute the rectangular layout for each leaf pane given the root area.
    pub fn compute_layout(&self, area: Rect) -> Vec<(usize, Rect)> {
        let mut result = Vec::new();
        Self::layout_node(&self.root, area, &mut result);
        result
    }

    fn layout_node(node: &SplitNode, area: Rect, out: &mut Vec<(usize, Rect)>) {
        match node {
            SplitNode::Leaf(pane) => {
                out.push((pane.id, area));
            }
            SplitNode::Container { direction, children, ratios } => {
                if children.is_empty() {
                    return;
                }
                let total_ratio: u16 = ratios.iter().sum::<u16>().max(1);

                match direction {
                    SplitDirection::Vertical => {
                        // Split horizontally across X (columns)
                        let mut current_x = area.x;
                        let available_w = area.width;

                        for (i, child) in children.iter().enumerate() {
                            let ratio = ratios.get(i).copied().unwrap_or(100 / children.len() as u16);
                            let is_last = i == children.len() - 1;
                            let w = if is_last {
                                (area.x + available_w).saturating_sub(current_x)
                            } else {
                                ((available_w as u32 * ratio as u32) / total_ratio as u32) as u16
                            };

                            let child_area = Rect {
                                x: current_x,
                                y: area.y,
                                width: w.max(1),
                                height: area.height,
                            };
                            Self::layout_node(child, child_area, out);
                            current_x += w;
                        }
                    }
                    SplitDirection::Horizontal => {
                        // Split vertically across Y (rows)
                        let mut current_y = area.y;
                        let available_h = area.height;

                        for (i, child) in children.iter().enumerate() {
                            let ratio = ratios.get(i).copied().unwrap_or(100 / children.len() as u16);
                            let is_last = i == children.len() - 1;
                            let h = if is_last {
                                (area.y + available_h).saturating_sub(current_y)
                            } else {
                                ((available_h as u32 * ratio as u32) / total_ratio as u32) as u16
                            };

                            let child_area = Rect {
                                x: area.x,
                                y: current_y,
                                width: area.width,
                                height: h.max(1),
                            };
                            Self::layout_node(child, child_area, out);
                            current_y += h;
                        }
                    }
                }
            }
        }
    }

    /// Select pane containing coordinate (x, y).
    pub fn select_pane_at(&mut self, x: u16, y: u16, area: Rect) -> bool {
        let layout = self.compute_layout(area);
        for (id, rect) in layout {
            if x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height {
                self.active_pane_id = id;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_tree_basic() {
        let mut tree = SplitTree::new(0);
        assert_eq!(tree.pane_count(), 1);
        assert_eq!(tree.active_pane_id, 0);

        let p2 = tree.split_active(SplitDirection::Vertical, Some(1));
        assert_eq!(tree.pane_count(), 2);
        assert_eq!(tree.active_pane_id, p2);

        let p3 = tree.split_active(SplitDirection::Horizontal, Some(2));
        assert_eq!(tree.pane_count(), 3);
        assert_eq!(tree.active_pane_id, p3);

        tree.cycle_pane(true);
        assert_ne!(tree.active_pane_id, p3);

        let closed = tree.close_active_pane();
        assert!(closed);
        assert_eq!(tree.pane_count(), 2);

        tree.only_active_pane();
        assert_eq!(tree.pane_count(), 1);
    }
}
