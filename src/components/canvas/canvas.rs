use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;

use crate::components::canvas::geometry::{
    find_input_port_at, get_node_id_from_event, is_port, is_text_input, is_trigger_button,
};
use crate::components::canvas::state::{
    compute_port_offsets, get_output_ports, get_port_canvas_position, ConnectionState,
    DraggingConnection, NodeState, Port, PortDirection, PortType, QueryTranslationMode,
};
use crate::components::canvas::wires::draw_connections;
use crate::components::nodes::node::GraphNode;

/// Check if two ports are compatible for connection
fn ports_compatible(source: &Port, target: &Port) -> bool {
    // Trigger ports only connect to other trigger ports
    if source.port_type == PortType::Trigger || target.port_type == PortType::Trigger {
        return source.port_type == target.port_type;
    }
    // All other port types can connect to each other
    true
}

/// Canvas for rendering nodes with pan/zoom
#[component]
pub fn Canvas(
    /// Selected node IDs
    selected_node_ids: Signal<HashSet<u32>>,
    set_selected_node_ids: WriteSignal<HashSet<u32>>,
    /// All nodes
    nodes: Signal<Vec<NodeState>>,
    set_nodes: WriteSignal<Vec<NodeState>>,
    /// All connections
    connections: Signal<Vec<ConnectionState>>,
    set_connections: WriteSignal<Vec<ConnectionState>>,
    /// Node ID currently being deleted (for shrink animation)
    #[prop(default = None)]
    deleting_node_id: Option<Signal<Option<u32>>>,
    /// Callback when node selection changes
    #[prop(default = None)]
    on_selection_change: Option<Callback<Option<u32>>>,
    /// Callback when a node is dropped from the palette (receives node_type, canvas x, y)
    #[prop(default = None)]
    on_node_drop: Option<Callback<(String, f64, f64)>>,
    /// Callback when a bundled group is dropped on the canvas (receives bundle_id, canvas x, y)
    #[prop(default = None)]
    on_bundle_drop: Option<Callback<(String, f64, f64)>>,
    /// Callback when a saved selection is dropped on the canvas (receives selection_id, canvas x, y)
    #[prop(default = None)]
    on_selection_drop: Option<Callback<(String, f64, f64)>>,
    /// Current zoom level
    zoom: Signal<f64>,
    /// Zoom setter
    set_zoom: WriteSignal<f64>,
    /// Left panel width signal (for calculating canvas offset)
    #[prop(default = None)]
    left_width: Option<Signal<i32>>,
    /// Right panel width signal (for canvas redraw on resize)
    #[prop(default = None)]
    right_width: Option<Signal<i32>>,
    /// Inspector panel height signal (for canvas redraw on resize)
    #[prop(default = None)]
    inspector_height: Option<Signal<i32>>,
    /// Callback when trigger node is clicked
    #[prop(default = None)]
    on_trigger: Option<Callback<u32>>,
    /// Callback when text input changes in a node
    #[prop(default = None)]
    on_text_change: Option<Callback<(u32, String)>>,
    /// Callback when limit changes in a Retrieval node
    #[prop(default = None)]
    on_limit_change: Option<Callback<(u32, usize)>>,
    /// Callback when query translation mode changes
    #[prop(default = None)]
    on_mode_change: Option<Callback<(u32, QueryTranslationMode)>>,
    /// Callback when a node is right-clicked for inspection
    /// Args: (node_id, is_double_click)
    #[prop(default = None)]
    on_node_right_click: Option<Callback<(u32, bool)>>,
    /// Called when a continuous interaction starts (drag, selection, connection drag)
    #[prop(default = None)]
    on_interaction_start: Option<Callback<()>>,
    /// Called when a continuous interaction ends
    #[prop(default = None)]
    on_interaction_end: Option<Callback<()>>,
) -> impl IntoView {
    // Canvas transform state (local to canvas)
    let (pan_x, set_pan_x) = signal(0.0f64);
    let (pan_y, set_pan_y) = signal(0.0f64);

    // Resize counter — incremented on window resize to force redraw
    let (resize_gen, set_resize_gen) = signal(0u32);

    // Track dragging state
    let (is_panning, set_is_panning) = signal(false);
    let (last_mouse_x, set_last_mouse_x) = signal(0.0f64);
    let (last_mouse_y, set_last_mouse_y) = signal(0.0f64);

    // Node dragging state
    let (dragging_node_id, set_dragging_node_id) = signal(Option::<u32>::None);
    let (drag_offset_x, set_drag_offset_x) = signal(0.0f64);
    let (drag_offset_y, set_drag_offset_y) = signal(0.0f64);
    let (drag_initial_positions, set_drag_initial_positions) =
        signal(HashMap::<u32, (f64, f64)>::new()); // node_id -> (initial_x, initial_y)

    // Connection state (local to canvas - wires are drawn here)
    let (dragging_connection, set_dragging_connection) = signal(Option::<DraggingConnection>::None);
    let (rerouting_from, set_rerouting_from) = signal(Option::<u32>::None);
    let (next_connection_id, set_next_connection_id) = signal(1u32);

    // Rubber-band selection state
    let (is_selecting, set_is_selecting) = signal(false);
    let (selection_box, set_selection_box) = signal(Option::<(f64, f64, f64, f64)>::None); // (start_x, start_y, end_x, end_y)
    let (selection_drag_start, set_selection_drag_start) = signal(Option::<(f64, f64)>::None); // canvas coords of mousedown

    // Get canvas offset (left panel width + divider width)
    let get_canvas_offset_x =
        move || -> f64 { left_width.map(|w| w.get() as f64 + 4.0).unwrap_or(264.0) };

    // Zoom controls
    let zoom_in = move |_| {
        let current = zoom.get();
        if current < 4.0 {
            set_zoom.set(current + 0.1);
        }
    };

    let zoom_out = move |_| {
        let current = zoom.get();
        if current > 0.25 {
            set_zoom.set(current - 0.1);
        }
    };

    let reset_zoom = move |_| {
        set_zoom.set(1.0);
        set_pan_x.set(0.0);
        set_pan_y.set(0.0);
    };

    // Compute port positions as a memoized HashMap keyed by (node_id, port_name)
    let port_positions = Memo::new(move |_| {
        let nodes = nodes.get();
        let mut positions: HashMap<(u32, String), (f64, f64)> = HashMap::new();
        for node in &nodes {
            let input_ports: Vec<_> = node
                .ports
                .iter()
                .filter(|p| p.direction == PortDirection::In)
                .cloned()
                .collect();
            let output_ports = get_output_ports(&node.node_type, &node.variant);
            let all_ports: Vec<_> = input_ports
                .into_iter()
                .chain(output_ports.into_iter())
                .collect();
            let ports_with_offsets = compute_port_offsets(&all_ports);
            for pwo in &ports_with_offsets {
                let (x, y) = get_port_canvas_position(
                    node.x,
                    node.y,
                    pwo.port.direction.clone(),
                    pwo.top_offset,
                );
                positions.insert((node.id, pwo.port.name.clone()), (x, y));
            }
        }
        positions
    });

    // Get port center position from memo
    let get_port_center = move |node_id: u32, port_name: &str| -> (f64, f64) {
        port_positions
            .get()
            .get(&(node_id, port_name.to_string()))
            .copied()
            .unwrap_or((0.0, 0.0))
    };

    // Port drag handlers
    let handle_output_drag_start =
        move |node_id: u32, port_name: String, _mouse_x: f64, _mouse_y: f64| {
            let (sx, sy) = get_port_center(node_id, &port_name);
            set_dragging_connection.set(Some(DraggingConnection {
                source_node_id: node_id,
                source_port_name: port_name.clone(),
                source_input_node_id: None,
                current_x: sx,
                current_y: sy,
                is_dragging: false,
            }));
            if let Some(callback) = &on_interaction_start {
                callback.run(());
            }
        };

    let handle_input_drag_end = move |node_id: u32, target_port_name: String, _x: f64, _y: f64| {
        if let Some(dc) = dragging_connection.get() {
            if dc.source_node_id != node_id {
                // Get source and target nodes
                let all_nodes = nodes.get();
                let source_node = all_nodes.iter().find(|n| n.id == dc.source_node_id);

                let target_node = all_nodes.iter().find(|n| n.id == node_id);

                // Validate port compatibility - the SPECIFIC ports being connected must be compatible
                let is_compatible = source_node
                    .and_then(|s| {
                        target_node.map(|t| {
                            // Get the specific source port being dragged from
                            let src_output_ports = get_output_ports(&s.node_type, &s.variant);
                            let src_port = src_output_ports
                                .iter()
                                .find(|p| p.name == dc.source_port_name);
                            // Get the specific target port being connected to
                            let tgt_port = t
                                .ports
                                .iter()
                                .filter(|p| p.direction == PortDirection::In)
                                .find(|p| p.name == target_port_name);
                            // Both ports must exist and be compatible
                            src_port
                                .and_then(|sp| tgt_port.map(|tp| ports_compatible(sp, tp)))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                if !is_compatible {
                    // Invalid connection - cancel the drag
                    set_dragging_connection.set(None);
                    set_rerouting_from.set(None);
                    return;
                }

                if let Some(src_input) = dc.source_input_node_id {
                    set_connections.update(|c: &mut Vec<ConnectionState>| {
                        c.retain(|conn| {
                            !(conn.source_node_id == dc.source_node_id
                                && conn.target_node_id == src_input)
                        })
                    });
                }
                set_connections.update(|c: &mut Vec<ConnectionState>| {
                    c.retain(|conn| {
                        !(conn.target_node_id == node_id
                            && conn.target_port_name == target_port_name)
                    })
                });
                let new_conn = ConnectionState {
                    id: next_connection_id.get(),
                    source_node_id: dc.source_node_id,
                    source_port_name: dc.source_port_name.clone(),
                    target_node_id: node_id,
                    target_port_name,
                    selected: false,
                };
                set_connections.update(|c: &mut Vec<ConnectionState>| c.push(new_conn));
                set_next_connection_id.update(|n| *n += 1);
            }
        }
        set_dragging_connection.set(None);
        set_rerouting_from.set(None);
    };

    let handle_input_click: Callback<(u32,)> = Callback::new(move |args: (u32,)| {
        set_connections
            .update(|c: &mut Vec<ConnectionState>| c.retain(|conn| conn.target_node_id != args.0));
    });

    let handle_input_reroute_start: Callback<(u32,)> = Callback::new(move |args: (u32,)| {
        let all_connections = connections.get();
        let existing_conn = all_connections.iter().find(|c| c.target_node_id == args.0);

        if let Some(conn) = existing_conn {
            let src_id = conn.source_node_id;
            let src_port_name = conn.source_port_name.clone();
            let (sx, sy) = get_port_center(src_id, &src_port_name);
            set_rerouting_from.set(Some(args.0));
            set_dragging_connection.set(Some(DraggingConnection {
                source_node_id: src_id,
                source_port_name: src_port_name,
                source_input_node_id: Some(args.0),
                current_x: sx,
                current_y: sy,
                is_dragging: false,
            }));
            if let Some(callback) = &on_interaction_start {
                callback.run(());
            }
        }
    });

    let cancel_connection_drag: Callback<(), ()> = Callback::new(move |_args: ()| {
        let had_connection = dragging_connection.get().is_some();
        set_dragging_connection.set(None);
        set_rerouting_from.set(None);
        if had_connection {
            if let Some(callback) = &on_interaction_end {
                callback.run(());
            }
        }
    });

    let handle_node_right_click = move |node_id: u32, is_double: bool| {
        if let Some(callback) = &on_node_right_click {
            callback.run((node_id, is_double));
        }
    };

    // Pan handling
    let handle_mouse_down = move |ev: web_sys::MouseEvent| {
        // Right-click to pan canvas
        if ev.button() == 2 {
            if let Some(dc) = dragging_connection.get() {
                if dc.is_dragging {
                    set_dragging_connection.set(None);
                    set_rerouting_from.set(None);
                    if let Some(callback) = &on_interaction_end {
                        callback.run(());
                    }
                    return;
                }
            }
            set_is_panning.set(true);
            set_last_mouse_x.set(ev.client_x() as f64);
            set_last_mouse_y.set(ev.client_y() as f64);
            return;
        }

        // Left-click handling
        if ev.button() == 0 {
            if is_port(&ev) {
                return;
            }

            if let Some(node_id) = get_node_id_from_event(&ev) {
                let canvas_offset_x = get_canvas_offset_x();
                let canvas_offset_y = 0.0;
                let pan = pan_x.get();
                let pan_y_val = pan_y.get();
                let zoom_val = zoom.get();

                let canvas_x = (ev.client_x() as f64 - canvas_offset_x - pan) / zoom_val;
                let canvas_y = (ev.client_y() as f64 - canvas_offset_y - pan_y_val) / zoom_val;

                // Check if this is a trigger button click or text input - if so, don't select or drag
                if is_trigger_button(&ev) || is_text_input(&ev) {
                    return;
                }

                let is_shift = ev.shift_key();
                let already_selected = selected_node_ids.get().contains(&node_id);

                if is_shift {
                    // Shift+click: toggle node in selection
                    set_selected_node_ids.update(|ids| {
                        if ids.contains(&node_id) {
                            ids.remove(&node_id);
                        } else {
                            ids.insert(node_id);
                        }
                    });
                } else if already_selected {
                    // Already selected - keep multi-selection intact
                } else {
                    // Normal click: replace selection with this node
                    set_selected_node_ids.update(|ids| {
                        ids.clear();
                        ids.insert(node_id);
                    });
                }
                if let Some(callback) = on_selection_change {
                    callback.run(Some(node_id));
                }

                // Start dragging if node is selected AND shift is not held
                // (when shift is held, we only toggle selection, don't drag)
                if !ev.shift_key() {
                    let selected_ids = selected_node_ids.get();
                    if selected_ids.contains(&node_id) {
                        if let Some(node) = nodes.get().iter().find(|n| n.id == node_id) {
                            set_drag_offset_x.set(canvas_x - node.x);
                            set_drag_offset_y.set(canvas_y - node.y);
                        }
                        // Store initial positions of all selected nodes for multi-node drag
                        let initial_positions: HashMap<u32, (f64, f64)> = nodes
                            .get()
                            .iter()
                            .filter(|n| selected_ids.contains(&n.id))
                            .map(|n| (n.id, (n.x, n.y)))
                            .collect();
                        set_drag_initial_positions.set(initial_positions);
                        set_dragging_node_id.set(Some(node_id));
                        set_is_panning.set(false);
                        if let Some(callback) = &on_interaction_start {
                            callback.run(());
                        }
                        return;
                    }
                }
            } else {
                // Clicked on empty canvas - start rubber-band selection (no shift needed)
                let canvas_offset_x = get_canvas_offset_x();
                let canvas_offset_y = 0.0;
                let pan = pan_x.get();
                let pan_y_val = pan_y.get();
                let zoom_val = zoom.get();
                let canvas_x = (ev.client_x() as f64 - canvas_offset_x - pan) / zoom_val;
                let canvas_y = (ev.client_y() as f64 - canvas_offset_y - pan_y_val) / zoom_val;
                set_selection_drag_start.set(Some((canvas_x, canvas_y)));
                set_is_selecting.set(true);
                set_selection_box.set(Some((canvas_x, canvas_y, canvas_x, canvas_y)));
                if let Some(callback) = &on_interaction_start {
                    callback.run(());
                }
            }
        }
    };

    let handle_mouse_move = move |ev: web_sys::MouseEvent| {
        // Handle rubber-band selection
        if is_selecting.get() {
            if let Some((start_x, start_y)) = selection_drag_start.get() {
                let canvas_offset_x = get_canvas_offset_x();
                let canvas_offset_y = 0.0;
                let pan = pan_x.get();
                let pan_y_val = pan_y.get();
                let zoom_val = zoom.get();
                let canvas_x = (ev.client_x() as f64 - canvas_offset_x - pan) / zoom_val;
                let canvas_y = (ev.client_y() as f64 - canvas_offset_y - pan_y_val) / zoom_val;
                set_selection_box.set(Some((start_x, start_y, canvas_x, canvas_y)));
            }
        }

        // Handle node dragging (move all selected nodes)
        if let Some(_node_id) = dragging_node_id.get() {
            let canvas_offset_x = get_canvas_offset_x();
            let canvas_offset_y = 0.0;
            let pan = pan_x.get();
            let pan_y_val = pan_y.get();
            let zoom_val = zoom.get();

            let canvas_x = (ev.client_x() as f64 - canvas_offset_x - pan) / zoom_val;
            let canvas_y = (ev.client_y() as f64 - canvas_offset_y - pan_y_val) / zoom_val;

            let selected_ids = selected_node_ids.get();
            let drag_offset_x = drag_offset_x.get();
            let drag_offset_y = drag_offset_y.get();
            let initial_positions = drag_initial_positions.get();

            // Calculate the position where the clicked node should be now
            let clicked_node_new_x = canvas_x - drag_offset_x;
            let clicked_node_new_y = canvas_y - drag_offset_y;

            // Find the clicked node's initial position to calculate delta
            if let Some(dragging_id) = dragging_node_id.get() {
                if let Some((init_x, init_y)) = initial_positions.get(&dragging_id) {
                    let delta_x = clicked_node_new_x - init_x;
                    let delta_y = clicked_node_new_y - init_y;

                    // Move all selected nodes by the same delta, preserving their relative positions
                    set_nodes.update(|nodes: &mut Vec<NodeState>| {
                        for node in nodes.iter_mut() {
                            if selected_ids.contains(&node.id) {
                                if let Some((orig_x, orig_y)) = initial_positions.get(&node.id) {
                                    node.x = orig_x + delta_x;
                                    node.y = orig_y + delta_y;
                                }
                            }
                        }
                    });
                }
            }
            return;
        }

        let is_dragging = dragging_connection
            .get()
            .map(|dc| dc.is_dragging)
            .unwrap_or(false);

        if !is_dragging && dragging_connection.get().is_some() {
            set_is_panning.set(false);
            set_dragging_connection.update(|d| {
                if let Some(ref mut d) = d {
                    d.is_dragging = true;
                }
            });
        }

        if is_panning.get() {
            let dx = ev.client_x() as f64 - last_mouse_x.get();
            let dy = ev.client_y() as f64 - last_mouse_y.get();
            set_last_mouse_x.set(ev.client_x() as f64);
            set_last_mouse_y.set(ev.client_y() as f64);
            set_pan_x.update(|x| *x += dx);
            set_pan_y.update(|y| *y += dy);
        }

        if dragging_connection.get().is_some() && dragging_connection.get().unwrap().is_dragging {
            let canvas_offset_x = get_canvas_offset_x();
            let canvas_offset_y = 0.0;
            let pan = pan_x.get();
            let pan_y_val = pan_y.get();
            let zoom_val = zoom.get();

            let canvas_x = (ev.client_x() as f64 - canvas_offset_x - pan) / zoom_val;
            let canvas_y = (ev.client_y() as f64 - canvas_offset_y - pan_y_val) / zoom_val;

            set_dragging_connection.update(|dc| {
                if let Some(ref mut d) = dc {
                    d.current_x = canvas_x;
                    d.current_y = canvas_y;
                }
            });
        }
    };

    // Helper to get dragged node type from window
    let get_dragged_node_type = || -> Option<String> {
        let window = web_sys::window()?;
        let val = js_sys::Reflect::get(&window, &"draggedNodeType".into()).ok()?;
        if val.is_undefined() || val.is_null() {
            None
        } else {
            val.as_string()
        }
    };

    // Helper to clear dragged node type
    let clear_dragged_node_type = || {
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::delete_property(&window, &"draggedNodeType".into());
        }
    };

    // Helper to get dragged bundle id from window
    let get_dragged_bundle_id = || -> Option<String> {
        let window = web_sys::window()?;
        let val = js_sys::Reflect::get(&window, &"draggedBundleId".into()).ok()?;
        if val.is_undefined() || val.is_null() {
            None
        } else {
            val.as_string()
        }
    };

    // Helper to clear dragged bundle id
    let clear_dragged_bundle_id = || {
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::delete_property(&window, &"draggedBundleId".into());
        }
    };

    // Helper to get dragged selection id from window
    let get_dragged_selection_id = || -> Option<String> {
        let window = web_sys::window()?;
        let val = js_sys::Reflect::get(&window, &"draggedSelectionId".into()).ok()?;
        if val.is_undefined() || val.is_null() {
            None
        } else {
            val.as_string()
        }
    };

    // Helper to clear dragged selection id
    let clear_dragged_selection_id = || {
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::delete_property(&window, &"draggedSelectionId".into());
        }
    };

    let handle_mouse_up = move |ev: web_sys::MouseEvent| {
        let was_interacting = dragging_node_id.get().is_some()
            || is_selecting.get()
            || dragging_connection.get().is_some();
        set_dragging_node_id.set(None);
        set_is_panning.set(false);

        let compute_drop_coords = || -> (f64, f64) {
            let canvas_offset_x = get_canvas_offset_x();
            let zoom_val = zoom.get();
            (
                (ev.client_x() as f64 - canvas_offset_x - pan_x.get()) / zoom_val,
                (ev.client_y() as f64 - pan_y.get()) / zoom_val,
            )
        };

        // Complete rubber-band selection
        if is_selecting.get() {
            set_is_selecting.set(false);
            if let Some((start_x, start_y, end_x, end_y)) = selection_box.get() {
                let min_x = start_x.min(end_x);
                let max_x = start_x.max(end_x);
                let min_y = start_y.min(end_y);
                let max_y = start_y.max(end_y);
                let node_width = 160.0;
                let node_height = 100.0;
                let selected: HashSet<u32> = nodes
                    .get()
                    .iter()
                    .filter(|n| {
                        // Partial coverage: any part of node overlapping selection box
                        n.x + node_width >= min_x
                            && n.x <= max_x
                            && n.y + node_height >= min_y
                            && n.y <= max_y
                    })
                    .map(|n| n.id)
                    .collect();
                set_selected_node_ids.set(selected);
                set_selection_box.set(None);
                set_selection_drag_start.set(None);
            }
        }

        // Check if a palette node is being dropped
        if let Some(callback) = &on_node_drop {
            if let Some(node_type) = get_dragged_node_type() {
                let (canvas_x, canvas_y) = compute_drop_coords();
                callback.run((node_type, canvas_x, canvas_y));
                clear_dragged_node_type();
            }
        }

        // Check if a bundled group is being dropped
        if let Some(callback) = &on_bundle_drop {
            if let Some(bundle_id) = get_dragged_bundle_id() {
                let (canvas_x, canvas_y) = compute_drop_coords();
                callback.run((bundle_id, canvas_x, canvas_y));
                clear_dragged_bundle_id();
            }
        }

        // Check if a saved selection is being dropped
        if let Some(callback) = &on_selection_drop {
            if let Some(selection_id) = get_dragged_selection_id() {
                let (canvas_x, canvas_y) = compute_drop_coords();
                callback.run((selection_id, canvas_x, canvas_y));
                clear_dragged_selection_id();
            }
        }

        if let Some(dc) = dragging_connection.get() {
            if dc.is_dragging {
                let target = find_input_port_at(ev.client_x() as f64, ev.client_y() as f64);
                if target.is_none() {
                    if let Some(src_input) = dc.source_input_node_id {
                        set_connections.update(|c: &mut Vec<ConnectionState>| {
                            c.retain(|conn| {
                                !(conn.source_node_id == dc.source_node_id
                                    && conn.target_node_id == src_input)
                            })
                        });
                    }
                    set_dragging_connection.set(None);
                    set_rerouting_from.set(None);
                }
            } else {
                // Clicked but didn't drag - cancel the connection
                set_dragging_connection.set(None);
                set_rerouting_from.set(None);
            }
        }
        if was_interacting {
            if let Some(callback) = &on_interaction_end {
                callback.run(());
            }
        }
    };

    let handle_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let delta = ev.delta_y();
        let current_zoom = zoom.get();
        let new_zoom = if delta < 0.0 {
            (current_zoom + 0.1).min(4.0)
        } else {
            (current_zoom - 0.1).max(0.25)
        };

        if new_zoom == current_zoom {
            return;
        }

        let canvas_offset_x = get_canvas_offset_x();
        let canvas_offset_y = 0.0;
        let cursor_x = ev.client_x() as f64;
        let cursor_y = ev.client_y() as f64;

        let canvas_x_before = (cursor_x - canvas_offset_x - pan_x.get()) / current_zoom;
        let canvas_y_before = (cursor_y - canvas_offset_y - pan_y.get()) / current_zoom;

        set_zoom.set(new_zoom);

        let new_pan_x = cursor_x - canvas_offset_x - canvas_x_before * new_zoom;
        let new_pan_y = cursor_y - canvas_offset_y - canvas_y_before * new_zoom;

        set_pan_x.set(new_pan_x);
        set_pan_y.set(new_pan_y);
    };

    let handle_canvas_dblclick = move |ev: web_sys::MouseEvent| {
        if get_node_id_from_event(&ev).is_some() {
            return;
        }

        if let Some(target) = ev.target() {
            if let Ok(element) = target.dyn_into::<web_sys::Element>() {
                if let Ok(Some(_)) = element.closest(".zoom-controls") {
                    return;
                }
            }
        }

        let nodes_snapshot = nodes.get();
        if nodes_snapshot.is_empty() {
            return;
        }

        let container = match ev.current_target() {
            Some(c) => c,
            None => return,
        };
        let container: web_sys::HtmlElement = match container.dyn_into() {
            Ok(c) => c,
            Err(_) => return,
        };
        let viewport_width = container.client_width() as f64;
        let viewport_height = container.client_height() as f64;

        let node_width = 160.0;
        let node_height = 100.0;

        let min_x = nodes_snapshot
            .iter()
            .map(|n| n.x)
            .fold(f64::INFINITY, f64::min);
        let min_y = nodes_snapshot
            .iter()
            .map(|n| n.y)
            .fold(f64::INFINITY, f64::min);
        let max_x = nodes_snapshot
            .iter()
            .map(|n| n.x + node_width)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = nodes_snapshot
            .iter()
            .map(|n| n.y + node_height)
            .fold(f64::NEG_INFINITY, f64::max);

        let content_width = max_x - min_x;
        let content_height = max_y - min_y;

        let padding = 0.1;
        let available_width = viewport_width * (1.0 - 2.0 * padding);
        let available_height = viewport_height * (1.0 - 2.0 * padding);

        let zoom_to_fit = (available_width / content_width)
            .min(available_height / content_height)
            .max(0.25)
            .min(4.0);

        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;

        let viewport_center_x = viewport_width / 2.0;
        let viewport_center_y = viewport_height / 2.0;

        let new_pan_x = viewport_center_x - center_x * zoom_to_fit;
        let new_pan_y = viewport_center_y - center_y * zoom_to_fit;

        set_zoom.set(zoom_to_fit);
        set_pan_x.set(new_pan_x);
        set_pan_y.set(new_pan_y);
    };

    let zoom_percent = move || format!("{}%", (zoom.get() * 100.0).round() as i32);

    let transform_style = move || {
        format!(
            "translate({}px, {}px) scale({})",
            pan_x.get(),
            pan_y.get(),
            zoom.get()
        )
    };

    // Track panel widths and inspector height for redraw
    let left_w = left_width;
    let right_w = right_width;
    let inspector_h = inspector_height;

    // Canvas redraw effect - re-runs when panel widths change (via left_w/right_w tracking)
    // Also tracks zoom/pan so moving/zooming triggers redraw
    // resize_gen forces redraw on window resize
    Effect::new(move |_| {
        // Track panel widths so Effect re-runs after panel resize
        let _lw = left_w.get();
        let _rw = right_w.get();
        // Track inspector height so Effect re-runs after inspector resize
        let _ih = inspector_h.get();
        // Track zoom/pan so moving/zooming triggers redraw
        let _zoom = zoom.get();
        let _pan_x = pan_x.get();
        let _pan_y = pan_y.get();
        // Track resize counter so Effect re-runs after window resize
        let _resize_gen = resize_gen.get();

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };
        let canvas_elem = match document.get_element_by_id("wires-canvas") {
            Some(e) => e,
            None => return,
        };
        let canvas_ref: web_sys::HtmlCanvasElement = match canvas_elem.dyn_into() {
            Ok(c) => c,
            Err(_) => return,
        };

        // Resize canvas to match container
        if let Some(container) = canvas_ref.parent_element() {
            let container: web_sys::HtmlElement = match container.dyn_into() {
                Ok(c) => c,
                Err(_) => return,
            };
            canvas_ref.set_width(container.client_width() as u32);
            canvas_ref.set_height(container.client_height() as u32);
        }

        let ctx: web_sys::CanvasRenderingContext2d = match canvas_ref.get_context("2d") {
            Ok(Some(c)) => c.unchecked_into(),
            _ => return,
        };

        let connections = connections.get();
        let dragging = dragging_connection.get();
        let rerouting = rerouting_from.get();
        let nodes = nodes.get();
        let port_pos = port_positions.get();
        draw_connections(
            &ctx,
            &connections,
            &dragging,
            rerouting,
            &nodes,
            &port_pos,
            pan_x.get(),
            pan_y.get(),
            zoom.get(),
        );
    });

    // Window resize listener — increments counter to force canvas redraw
    static RESIZE_LISTENER_ADDED: std::sync::Once = std::sync::Once::new();
    let set_resize_gen_clone = set_resize_gen.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev: web_sys::Event| {
        set_resize_gen_clone.update(|g| *g += 1);
    }) as Box<dyn Fn(_)>);

    RESIZE_LISTENER_ADDED.call_once(|| {
        if let Some(w) = web_sys::window() {
            w.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
                .ok();
        }
    });
    closure.forget();

    view! {
        <div
            class="canvas-container"
            on:mousedown={handle_mouse_down}
            on:mousemove={handle_mouse_move}
            on:mouseup={handle_mouse_up}
            on:mouseleave={handle_mouse_up}
            on:wheel={handle_wheel}
            on:dblclick={handle_canvas_dblclick}
            on:contextmenu={|ev| ev.prevent_default()}
        >
            <canvas
                id="wires-canvas"
                style:position="absolute"
                style:top="0"
                style:left="0"
                style:width="100%"
                style:height="100%"
                style:pointer-events="none"
                style:z_index="0"
            ></canvas>

            {/* Rubber-band selection box */}
            {move || {
                if let Some((start_x, start_y, end_x, end_y)) = selection_box.get() {
                    let zoom_val = zoom.get();
                    let pan_val = pan_x.get();
                    let pan_y_val = pan_y.get();
                    // Convert canvas coords to screen coords
                    let screen_x = start_x.min(end_x) * zoom_val + pan_val;
                    let screen_y = start_y.min(end_y) * zoom_val + pan_y_val;
                    let screen_w = (end_x - start_x).abs() * zoom_val;
                    let screen_h = (end_y - start_y).abs() * zoom_val;
                    view! {
                        <div
                            class="selection-box"
                            style:left={format!("{}px", screen_x)}
                            style:top={format!("{}px", screen_y)}
                            style:width={format!("{}px", screen_w)}
                            style:height={format!("{}px", screen_h)}
                        ></div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            <div
                class="canvas"
                style:transform={transform_style}
            >
                {move || {
                    let connections_snapshot = connections.get();
                    let selected_ids = selected_node_ids.get();
                    let deleting = deleting_node_id.and_then(|s| s.get());
                    nodes.get().iter().map(|node| {
                        let has_connection = connections_snapshot.iter().any(|c| c.target_node_id == node.id);
                        let is_selected = selected_ids.contains(&node.id);
                        let is_deleting = deleting == Some(node.id);
                        // Get input ports from node.ports (static), output ports from get_output_ports (dynamic)
                        let input_ports = node.ports.iter().filter(|p| p.direction == PortDirection::In).cloned().collect::<Vec<_>>();
                        let output_ports = get_output_ports(&node.node_type, &node.variant);
                        let all_ports: Vec<Port> = input_ports.into_iter().chain(output_ports.into_iter()).collect();
                        let combined_ports = compute_port_offsets(&all_ports);
                        view! {
                            <GraphNode
                                x={node.x}
                                y={node.y}
                                label={node.label.clone()}
                                selected={is_selected}
                                node_id={node.id}
                                variant={node.variant.clone()}
                                ports={combined_ports}
                                has_input_connection={has_connection}
                                is_deleting={is_deleting}
                                on_output_drag_start={Some(Callback::from(handle_output_drag_start))}
                                on_input_drag_end={Some(Callback::from(handle_input_drag_end))}
                                on_input_click={Some(handle_input_click)}
                                on_input_reroute_start={Some(Callback::from(handle_input_reroute_start))}
                                cancel_connection_drag={Some(cancel_connection_drag)}
                                on_trigger={on_trigger}
                                on_text_change={on_text_change}
                                on_limit_change={on_limit_change}
                                on_mode_change={on_mode_change}
                                on_node_right_click={Some(Callback::from(handle_node_right_click))}
                            />
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>

            <div class="zoom-controls">
                <button class="zoom-btn" on:click={zoom_out} title="Zoom out">"-"</button>
                <span class="zoom-level">{zoom_percent}</span>
                <button class="zoom-btn" on:click={zoom_in} title="Zoom in">"+"</button>
                <button class="zoom-btn" on:click={reset_zoom} title="Reset view">"⟲"</button>
            </div>
        </div>
    }
}
