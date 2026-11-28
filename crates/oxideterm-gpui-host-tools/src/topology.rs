mod graph;
pub use graph::{topology_canvas, topology_node};
use super::*;
use gpui::{MouseMoveEvent, ScrollEvent};
use oxideterm_topology::{TOPOLOGY_NODE_HEIGHT, TOPOLOGY_NODE_WIDTH, TopologyLayoutNode};

impl HostToolsEntity {
    pub fn topology_transform(&self) -> TopologyTransform {
        self.topology_transform
    }

    pub fn topology_dragging(&self) -> bool {
        self.topology_drag.is_some()
    }

    pub fn topology_menu(&self) -> Option<TopologyNodeMenuState> {
        self.topology_menu.clone()
    }

    pub fn dismiss_topology_menu(&mut self, cx: &mut Context<Self>) -> bool {
        // The menu contains only node display metadata and remains local to the
        // Host Tools Entity across main-tab and detached-window renderers.
        let changed = self.topology_menu.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn zoom_topology_graph(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) -> bool {
        let delta = event.delta.pixel_delta(px(16.0));
        let vertical = f32::from(delta.y);
        if vertical == 0.0 {
            return false;
        }

        let old = self.topology_transform;
        let wheel_factor = (1.0 - vertical * 0.001).clamp(0.85, 1.15);
        let next_k = (old.k * wheel_factor).clamp(TOPOLOGY_ZOOM_MIN, TOPOLOGY_ZOOM_MAX);
        if (next_k - old.k).abs() < f32::EPSILON {
            return false;
        }

        let cursor_x = f32::from(event.position.x);
        let cursor_y = f32::from(event.position.y);
        let graph_x = (cursor_x - old.x) / old.k;
        let graph_y = (cursor_y - old.y) / old.k;
        self.topology_transform = TopologyTransform {
            x: cursor_x - graph_x * next_k,
            y: cursor_y - graph_y * next_k,
            k: next_k,
        };
        cx.notify();
        true
    }

    pub fn begin_topology_drag(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.topology_drag = Some(TopologyDragState {
            last_x: f32::from(event.position.x),
            last_y: f32::from(event.position.y),
        });
        cx.notify();
    }

    pub fn pan_topology_graph(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.topology_drag else {
            return false;
        };
        if !event.dragging() {
            return false;
        }

        let x = f32::from(event.position.x);
        let y = f32::from(event.position.y);
        let dx = x - drag.last_x;
        let dy = y - drag.last_y;
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        self.topology_transform.x += dx;
        self.topology_transform.y += dy;
        self.topology_drag = Some(TopologyDragState {
            last_x: x,
            last_y: y,
        });
        cx.notify();
        true
    }

    pub fn end_topology_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.topology_drag.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn open_topology_node_menu(
        &mut self,
        node_id: Option<NodeId>,
        node: &TopologyLayoutNode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let transform = self.topology_transform;
        let window_bounds = window.inner_window_bounds().get_bounds();
        let max_x = (f32::from(window_bounds.size.width) - TOPOLOGY_MENU_WIDTH).max(0.0);
        let max_y = (f32::from(window_bounds.size.height) - TOPOLOGY_MENU_MAX_HEIGHT).max(0.0);
        let x = (topology_transform_x(node.x, transform)
            + TOPOLOGY_NODE_WIDTH * transform.k / 2.0
            + 8.0)
            .min(max_x)
            .max(0.0);
        let y = (topology_transform_y(node.y, transform)
            - TOPOLOGY_NODE_HEIGHT * transform.k / 2.0)
            .min(max_y)
            .max(0.0);

        self.topology_menu = Some(TopologyNodeMenuState {
            node_id,
            name: node.name.clone(),
            host: node.host.clone(),
            view_status: node.view_status,
            x,
            y,
        });
        cx.notify();
    }
}
