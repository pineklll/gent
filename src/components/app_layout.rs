use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::components::canvas::geometry::is_text_input_keyboard;
use crate::components::canvas::state::{
    default_ports_for_type, default_variant_for_type, BundledGroup, ConnectionState, NodeState,
    NodeStatus, NodeVariant, SavedSelection,
};
use crate::components::canvas::Canvas;
use crate::components::execution_engine::{
    execute_downstream_order, execute_node_sync, ExecutionState,
};
use crate::components::inspector_panel::{InspectorPanel, InspectorTab};
use crate::components::left_panel::{LeftPanel, NODE_TYPES};
use crate::components::right_panel::RightPanel;
use crate::components::graph_section::BUNDLED_GROUPS;
use crate::components::save_load::{
    copy_to_clipboard, export_to_file, generate_id, import_from_file, load_selection,
    paste_from_clipboard, save_saved_selections_to_storage,
};
use crate::components::toast::{Toast, ToastContainer, ToastType};
use crate::components::undo::{GraphSnapshot, UndoManager};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmOutput {
    pub text: String,
    pub tokens_used: u32,
    pub model: String,
    pub finish_reason: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ProviderConfig {
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct LlmDefaults {
    pub default_format: Option<String>,
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfig>,
}

async fn fetch_llm_defaults() -> Result<LlmDefaults, String> {
    use crate::tauri_invoke;
    let opts = js_sys::Object::new();
    let js_value = tauri_invoke::invoke("get_llm_defaults".into(), &opts).await?;
    serde_wasm_bindgen::from_value(js_value).map_err(|e| format!("deserialization failed: {:?}", e))
}

fn apply_llm_defaults_to_bundle(bundle: &mut BundledGroup, defaults: &LlmDefaults) {
    use crate::components::canvas::state::NodeVariant;
    for node in &mut bundle.nodes {
        if let NodeVariant::ModelConfig {
            format,
            model_name,
            api_key,
            custom_url,
        } = &mut node.variant
        {
            if let Some(ref default_fmt) = defaults.default_format {
                format.clone_from(default_fmt);
            }
            if let Some(provider) = defaults.providers.get(format) {
                if model_name.is_empty() {
                    if let Some(ref m) = provider.model {
                        model_name.clone_from(m);
                    }
                }
                if api_key.is_empty() {
                    if let Some(ref k) = provider.api_key {
                        api_key.clone_from(k);
                    }
                }
                if custom_url.is_empty() {
                    if let Some(ref e) = provider.endpoint {
                        custom_url.clone_from(e);
                    }
                }
            }
        }
    }
}

/// Call Tauri backend for LLM completion
async fn call_llm_complete(
    format: String,
    model_name: String,
    api_key: String,
    custom_url: String,
    prompt: String,
    temperature: f64,
) -> Result<LlmOutput, String> {
    use crate::tauri_invoke;
    let opts = js_sys::Object::new();
    let config = js_sys::Object::new();
    let config_js: JsValue = config.into();
    if !js_sys::Reflect::set(&opts, &"config".into(), &config_js).unwrap_or(false) {
        return Err("Failed to set config".to_string());
    }
    if !js_sys::Reflect::set(&config_js, &"format".into(), &format.into()).unwrap_or(false) {
        return Err("Failed to set format".to_string());
    }
    if !js_sys::Reflect::set(&config_js, &"model_name".into(), &model_name.into()).unwrap_or(false)
    {
        return Err("Failed to set model_name".to_string());
    }
    if !js_sys::Reflect::set(&config_js, &"api_key".into(), &api_key.into()).unwrap_or(false) {
        return Err("Failed to set api_key".to_string());
    }
    if !js_sys::Reflect::set(&config_js, &"custom_url".into(), &custom_url.into()).unwrap_or(false)
    {
        return Err("Failed to set custom_url".to_string());
    }
    let input = js_sys::Object::new();
    let input_js: JsValue = input.into();
    if !js_sys::Reflect::set(&opts, &"input".into(), &input_js).unwrap_or(false) {
        return Err("Failed to set input".to_string());
    }
    if !js_sys::Reflect::set(&input_js, &"prompt".into(), &prompt.into()).unwrap_or(false) {
        return Err("Failed to set prompt".to_string());
    }
    if !js_sys::Reflect::set(
        &input_js,
        &"temperature".into(),
        &JsValue::from_f64(temperature),
    )
    .unwrap_or(false)
    {
        return Err("Failed to set temperature".to_string());
    }
    let js_value = tauri_invoke::invoke("llm_complete".into(), &opts).await?;
    serde_wasm_bindgen::from_value(js_value).map_err(|e| format!("deserialization failed: {:?}", e))
}

async fn call_retrieve(
    collection: String,
    query: String,
    top_k: usize,
    group_id: String,
    api_key: String,
    output_type: String,
) -> Result<Vec<crate::components::execution_engine::RetrievalResult>, String> {
    use crate::tauri_invoke;
    let opts = js_sys::Object::new();
    if !js_sys::Reflect::set(&opts, &"collection".into(), &collection.into()).unwrap_or(false) {
        return Err("Failed to set collection".to_string());
    }
    if !js_sys::Reflect::set(&opts, &"query".into(), &query.into()).unwrap_or(false) {
        return Err("Failed to set query".to_string());
    }
    if !js_sys::Reflect::set(&opts, &"top_k".into(), &JsValue::from_f64(top_k as f64)).unwrap_or(false) {
        return Err("Failed to set top_k".to_string());
    }
    if !js_sys::Reflect::set(&opts, &"group_id".into(), &group_id.into()).unwrap_or(false) {
        return Err("Failed to set group_id".to_string());
    }
    if !js_sys::Reflect::set(&opts, &"api_key".into(), &api_key.into()).unwrap_or(false) {
        return Err("Failed to set api_key".to_string());
    }
    if !js_sys::Reflect::set(&opts, &"output_type".into(), &output_type.into()).unwrap_or(false) {
        return Err("Failed to set output_type".to_string());
    }
    let js_value = tauri_invoke::invoke("retrieve".into(), &opts).await?;
    let results: Vec<crate::components::execution_engine::RetrievalResult> =
        serde_wasm_bindgen::from_value(js_value)
            .map_err(|e| format!("deserialization failed: {:?}", e))?;
    Ok(results)
}

/// Main application layout with left panel, canvas, and right panel
fn shift_nodes(nodes: &mut [NodeState], dx: f64, dy: f64) {
    for n in nodes {
        n.x += dx;
        n.y += dy;
    }
}

fn graph_bounds(nodes: &[NodeState]) -> (f64, f64) {
    if nodes.is_empty() {
        return (160.0, 100.0);
    }
    let min_x = nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let min_y = nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    let max_x = nodes.iter().map(|n| n.x).fold(f64::NEG_INFINITY, f64::max);
    let max_y = nodes.iter().map(|n| n.y).fold(f64::NEG_INFINITY, f64::max);
    let width = (max_x + 160.0) - min_x;
    let height = (max_y + 100.0) - min_y;
    (width.max(0.0), height.max(0.0))
}

fn center_nodes_at(nodes: &mut [NodeState], x: f64, y: f64) {
    let (width, height) = graph_bounds(nodes);
    let min_x = nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let min_y = nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    shift_nodes(nodes, x - min_x - width / 2.0, y - min_y - height / 2.0);
}

fn bundle_to_selection(bundle: BundledGroup) -> SavedSelection {
    SavedSelection {
        id: bundle.id.to_string(),
        name: bundle.name.to_string(),
        created_at: 0.0,
        nodes: bundle.nodes,
        connections: bundle.connections,
    }
}

#[component]
pub fn AppLayout() -> impl IntoView {
    // Shared state for panel sizes (in pixels)
    let (left_width, set_left_width) = signal(260i32);
    let (right_width, set_right_width) = signal(300i32);

    // Track if dragging divider
    let (dragging_left, set_dragging_left) = signal(false);
    let (dragging_right, set_dragging_right) = signal(false);

    // Inspector panel state
    let (inspector_tabs, set_inspector_tabs) = signal(Vec::<InspectorTab>::new());
    let (active_inspector_tab, set_active_inspector_tab) = signal(Option::<usize>::None);
    let (inspector_height, set_inspector_height) = signal(250i32);
    let (inspector_dragging, set_inspector_dragging) = signal(false);

    // Lifted state from Canvas for NodeInspector
    let (nodes, set_nodes) = signal(vec![
        NodeState {
            id: 1,
            x: 80.0,
            y: 150.0,
            node_type: "trigger".to_string(),
            label: "Trigger".to_string(),
            selected: false,
            status: NodeStatus::Pending,
            variant: default_variant_for_type("trigger"),
            ports: default_ports_for_type("trigger"),
        },
        NodeState {
            id: 2,
            x: 300.0,
            y: 150.0,
            node_type: "text_input".to_string(),
            label: "Text Input".to_string(),
            selected: false,
            status: NodeStatus::Pending,
            variant: default_variant_for_type("text_input"),
            ports: default_ports_for_type("text_input"),
        },
        NodeState {
            id: 3,
            x: 520.0,
            y: 150.0,
            node_type: "text_output".to_string(),
            label: "Text Output".to_string(),
            selected: false,
            status: NodeStatus::Pending,
            variant: default_variant_for_type("text_output"),
            ports: default_ports_for_type("text_output"),
        },
    ]);

    let (connections, set_connections) = signal(vec![
        ConnectionState {
            id: 1,
            source_node_id: 1,
            source_port_name: "output".to_string(),
            target_node_id: 2,
            target_port_name: "trigger".to_string(),
            selected: false,
        },
        ConnectionState {
            id: 2,
            source_node_id: 2,
            source_port_name: "output".to_string(),
            target_node_id: 3,
            target_port_name: "response".to_string(),
            selected: false,
        },
    ]);
    let (selected_node_ids, set_selected_node_ids) = signal(HashSet::<u32>::new());
    let (deleting_node_id, set_deleting_node_id) = signal(Option::<u32>::None);
    let (next_node_id, set_next_node_id) = signal(4u32);

    // Drag preview state
    let (dragging_node_type, set_dragging_node_type) = signal(Option::<String>::None);
    let (dragging_graph_label, set_dragging_graph_label) = signal(Option::<String>::None);
    let (drag_x, set_drag_x) = signal(0.0);
    let (drag_y, set_drag_y) = signal(0.0);
    let (drag_preview_w, set_drag_preview_w) = signal(0.0);
    let (drag_preview_h, set_drag_preview_h) = signal(0.0);
    let (zoom, set_zoom) = signal(1.0f64);

    // Execution state for the execution engine
    let (execution_state, set_execution_state) = signal(ExecutionState::new());

    // Keyboard shortcut state
    let (saved_selections, set_saved_selections) = signal(Vec::<SavedSelection>::new());
    let (toasts, set_toasts) = signal(Vec::<Toast>::new());
    let (next_toast_id, set_next_toast_id) = signal(0u32);
    let (next_connection_id, set_next_connection_id) = signal(100u32);
    let (load_offset_counter, set_load_offset_counter) = signal(0u32);
    let llm_defaults = StoredValue::new(LlmDefaults::default());

    // Undo/redo state
    let undo_manager = StoredValue::new(UndoManager::new());
    let is_undoing = StoredValue::new(false);
    let (undo_suppressed, set_undo_suppressed) = signal(false);
    let interaction_start_snapshot = StoredValue::new(None::<GraphSnapshot>);

    let capture_snapshot = move || GraphSnapshot {
        nodes: nodes.get(),
        connections: connections.get(),
        selected_node_ids: selected_node_ids.get(),
        next_node_id: next_node_id.get(),
        next_connection_id: next_connection_id.get(),
    };

    let last_snapshot = StoredValue::new(capture_snapshot());

    // Load LLM defaults from Tauri backend
    spawn_local({
        let llm_defaults = llm_defaults.clone();
        async move {
            if let Ok(defaults) = fetch_llm_defaults().await {
                llm_defaults.set_value(defaults);
            }
        }
    });

    // Snapshot effect: observes all undoable signals and pushes the previous state
    // onto the undo stack whenever a change occurs (unless the change was triggered
    // by an undo/redo operation).
    // During continuous interactions (drag, selection box, connection drag) pushes
    // are suppressed so the entire gesture becomes a single undo step.
    Effect::new(move |_| {
        let current = capture_snapshot();

        if is_undoing.get_value() {
            is_undoing.set_value(false);
            last_snapshot.set_value(current);
        } else if undo_suppressed.get() {
            // Swallow intermediate changes during drag/continuous interaction.
        } else {
            let prev = last_snapshot.get_value();
            if prev != current {
                undo_manager.update_value(|um| um.push(prev));
                last_snapshot.set_value(current);
            }
        }
    });

    let begin_undo_suppression = move || {
        if !undo_suppressed.get() {
            interaction_start_snapshot.set_value(Some(capture_snapshot()));
            set_undo_suppressed.set(true);
        }
    };

    let end_undo_suppression = move || {
        if undo_suppressed.get() {
            set_undo_suppressed.set(false);
            if let Some(start) = interaction_start_snapshot.get_value() {
                let current = capture_snapshot();
                if start != current {
                    undo_manager.update_value(|um| um.push(start));
                }
                last_snapshot.set_value(current);
            }
            interaction_start_snapshot.set_value(None);
        }
    };

    // Load saved selections on mount
    {
        let set_saved_selections = set_saved_selections.clone();
        spawn_local(async move {
            let loaded = crate::components::save_load::load_saved_selections();
            set_saved_selections.set(loaded);
        });
    }

    // Handler for text input changes
    let handle_text_change = move |node_id: u32, new_text: String| {
        set_nodes.update(|nodes: &mut Vec<NodeState>| {
            if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                if let crate::components::canvas::state::NodeVariant::UserInput { text } =
                    &mut node.variant
                {
                    *text = new_text;
                }
            }
        });
    };

    // Toast helper function
    let add_toast = {
        let set_toasts = set_toasts.clone();
        let next_toast_id = next_toast_id;
        let set_next_toast_id = set_next_toast_id;
        move |message: String, toast_type: ToastType| {
            let id = next_toast_id.get();
            set_next_toast_id.update(|n| *n += 1);
            set_toasts.update(|t| {
                t.push(Toast {
                    id,
                    message,
                    toast_type,
                })
            });
            let set_toasts_clone = set_toasts.clone();
            spawn_local(async move {
                TimeoutFuture::new(3000).await;
                set_toasts_clone.update(|t| t.retain(|toast| toast.id != id));
            });
        }
    };

    // Helper to apply a loaded selection/bundle to canvas state
    let apply_load_result = move |
        (new_nodes, new_conns, next_id, next_conn, new_ids):
        (Vec<NodeState>, Vec<ConnectionState>, u32, u32, HashSet<u32>),
        toast_msg: &str,
    | {
        set_nodes.update(|n| n.extend(new_nodes));
        set_connections.update(|c| c.extend(new_conns));
        set_next_node_id.set(next_id);
        set_next_connection_id.set(next_conn);
        set_selected_node_ids.set(new_ids);
        add_toast(toast_msg.to_string(), ToastType::Success);
    };

    // Undo / redo helpers
    let perform_undo = {
        let add_toast = add_toast.clone();
        move || {
            let current = capture_snapshot();
            if let Some(Some(snapshot)) = undo_manager.try_update_value(|um| um.undo(current)) {
                is_undoing.set_value(true);
                set_nodes.set(snapshot.nodes);
                set_connections.set(snapshot.connections);
                set_selected_node_ids.set(snapshot.selected_node_ids);
                set_next_node_id.set(snapshot.next_node_id);
                set_next_connection_id.set(snapshot.next_connection_id);
                add_toast("Undo".to_string(), ToastType::Info);
            }
        }
    };

    let perform_redo = {
        let add_toast = add_toast.clone();
        move || {
            let current = capture_snapshot();
            if let Some(Some(snapshot)) = undo_manager.try_update_value(|um| um.redo(current)) {
                is_undoing.set_value(true);
                set_nodes.set(snapshot.nodes);
                set_connections.set(snapshot.connections);
                set_selected_node_ids.set(snapshot.selected_node_ids);
                set_next_node_id.set(snapshot.next_node_id);
                set_next_connection_id.set(snapshot.next_connection_id);
                add_toast("Redo".to_string(), ToastType::Info);
            }
        }
    };

    // Non-passive window keydown listener
    static KEYDOWN_LISTENER_ADDED: std::sync::Once = std::sync::Once::new();
    let keydown_closure = wasm_bindgen::closure::Closure::wrap(Box::new({
        let connections = connections.clone();
        let nodes = nodes.clone();
        let selected_node_ids = selected_node_ids.clone();
        let set_selected_node_ids = set_selected_node_ids.clone();
        let set_nodes = set_nodes.clone();
        let set_connections = set_connections.clone();
        let set_deleting_node_id = set_deleting_node_id.clone();
        let _set_next_node_id = set_next_node_id.clone();
        let _set_next_connection_id = set_next_connection_id.clone();
        let next_node_id = next_node_id.clone();
        let next_connection_id = next_connection_id.clone();
        let saved_selections = saved_selections.clone();
        let set_saved_selections = set_saved_selections.clone();
        let add_toast = add_toast.clone();
        let on_selection_change = None::<Callback<Option<u32>>>;
        let perform_undo = perform_undo.clone();
        let perform_redo = perform_redo.clone();

        move |ev: web_sys::KeyboardEvent| {
            let ctrl = ev.ctrl_key() || ev.meta_key();
            let key = ev.key();

            // Ignore if focus is in text input
            if is_text_input_keyboard(&ev) {
                return;
            }

            match (ctrl, key.as_str()) {
                (true, "c") => {
                    // Copy selection to clipboard
                    ev.prevent_default();
                    let selected = selected_node_ids.get();
                    if selected.is_empty() {
                        return;
                    }
                    let nodes_snapshot = nodes.get();
                    let conns_snapshot = connections.get();
                    let selection = SavedSelection {
                        id: generate_id(),
                        name: "Selection".to_string(),
                        created_at: js_sys::Date::now(),
                        nodes: nodes_snapshot
                            .into_iter()
                            .filter(|n| selected.contains(&n.id))
                            .collect(),
                        connections: conns_snapshot
                            .into_iter()
                            .filter(|c| {
                                selected.contains(&c.source_node_id)
                                    && selected.contains(&c.target_node_id)
                            })
                            .collect(),
                    };
                    let add_toast_clone = add_toast.clone();
                    spawn_local(async move {
                        match copy_to_clipboard(selection, true).await {
                            Ok(_) => add_toast_clone(
                                "Copied to clipboard".to_string(),
                                ToastType::Success,
                            ),
                            Err(e) => {
                                add_toast_clone(format!("Copy failed: {}", e), ToastType::Error)
                            }
                        }
                    });
                }
                (true, "v") => {
                    // Paste from clipboard
                    ev.prevent_default();
                    let add_toast_clone = add_toast.clone();
                    let apply_load_result_clone = apply_load_result.clone();
                    spawn_local(async move {
                        match paste_from_clipboard().await {
                            Ok(selection) => {
                                let result = load_selection(
                                    selection,
                                    next_node_id.get(),
                                    next_connection_id.get(),
                                );
                                apply_load_result_clone(result, "Pasted from clipboard");
                            }
                            Err(e) => {
                                add_toast_clone(format!("Paste failed: {}", e), ToastType::Error)
                            }
                        }
                    });
                }
                (true, "s") => {
                    // Save selection
                    ev.prevent_default();
                    if selected_node_ids.get().is_empty() {
                        add_toast("No selection to save".to_string(), ToastType::Info);
                        return;
                    }
                    let selected = selected_node_ids.get();
                    let nodes_snapshot = nodes.get();
                    let conns_snapshot = connections.get();
                    let selection = SavedSelection {
                        id: generate_id(),
                        name: "Selection".to_string(),
                        created_at: js_sys::Date::now(),
                        nodes: nodes_snapshot
                            .into_iter()
                            .filter(|n| selected.contains(&n.id))
                            .collect(),
                        connections: conns_snapshot
                            .into_iter()
                            .filter(|c| {
                                selected.contains(&c.source_node_id)
                                    && selected.contains(&c.target_node_id)
                            })
                            .collect(),
                    };
                    let mut selections = saved_selections.get();
                    selections.push(selection.clone());
                    save_saved_selections_to_storage(&selections);
                    set_saved_selections.set(selections);
                    add_toast("Selection saved".to_string(), ToastType::Success);
                }
                (true, "e") => {
                    // Export selection to file
                    ev.prevent_default();
                    if selected_node_ids.get().is_empty() {
                        add_toast("No selection to export".to_string(), ToastType::Info);
                        return;
                    }
                    let selected = selected_node_ids.get();
                    let nodes_snapshot = nodes.get();
                    let conns_snapshot = connections.get();
                    let selection = SavedSelection {
                        id: generate_id(),
                        name: "Selection".to_string(),
                        created_at: js_sys::Date::now(),
                        nodes: nodes_snapshot
                            .into_iter()
                            .filter(|n| selected.contains(&n.id))
                            .collect(),
                        connections: conns_snapshot
                            .into_iter()
                            .filter(|c| {
                                selected.contains(&c.source_node_id)
                                    && selected.contains(&c.target_node_id)
                            })
                            .collect(),
                    };
                    let filename =
                        format!("{}.json", selection.name.to_lowercase().replace(" ", "_"));
                    let add_toast_clone = add_toast.clone();
                    spawn_local(async move {
                        match export_to_file(&selection, &filename).await {
                            Ok(_) => {
                                add_toast_clone("Exported to file".to_string(), ToastType::Success)
                            }
                            Err(e) => {
                                add_toast_clone(format!("Export failed: {}", e), ToastType::Error)
                            }
                        }
                    });
                }
                (true, "i") => {
                    // Import from file
                    ev.prevent_default();
                    let add_toast_clone = add_toast.clone();
                    let apply_load_result_clone = apply_load_result.clone();
                    spawn_local(async move {
                        match import_from_file().await {
                            Ok((selection, _name)) => {
                                let result = load_selection(
                                    selection,
                                    next_node_id.get(),
                                    next_connection_id.get(),
                                );
                                apply_load_result_clone(result, "Imported from file");
                            }
                            Err(e) => {
                                add_toast_clone(format!("Import failed: {}", e), ToastType::Error)
                            }
                        }
                    });
                }
                (true, "a") => {
                    // Select all
                    ev.prevent_default();
                    let all_ids: HashSet<u32> = nodes.get().iter().map(|n| n.id).collect();
                    set_selected_node_ids.set(all_ids);
                }
                (true, "z") => {
                    ev.prevent_default();
                    if ev.shift_key() {
                        perform_redo();
                    } else {
                        perform_undo();
                    }
                }
                (_, "Delete") | (_, "Backspace") => {
                    // Delete selected nodes
                    ev.prevent_default();
                    let to_delete = selected_node_ids.get();
                    if to_delete.is_empty() {
                        return;
                    }
                    // Animate and delete
                    if let Some(first_id) = to_delete.iter().next().copied() {
                        set_deleting_node_id.set(Some(first_id));
                    }
                    let to_delete_clone = to_delete.clone();
                    let set_nodes_clone = set_nodes.clone();
                    let set_connections_clone = set_connections.clone();
                    let set_deleting_node_id_clone = set_deleting_node_id.clone();
                    let set_selected_node_ids_clone = set_selected_node_ids.clone();
                    spawn_local(async move {
                        TimeoutFuture::new(300).await;
                        set_nodes_clone
                            .update(|n| n.retain(|node| !to_delete_clone.contains(&node.id)));
                        set_connections_clone.update(|c| {
                            c.retain(|conn| {
                                !to_delete_clone.contains(&conn.source_node_id)
                                    && !to_delete_clone.contains(&conn.target_node_id)
                            })
                        });
                        set_deleting_node_id_clone.set(None);
                        set_selected_node_ids_clone.update(|ids| ids.clear());
                    });
                }
                (_, "Escape") => {
                    // Clear selection
                    set_selected_node_ids.update(|ids| ids.clear());
                    if let Some(callback) = on_selection_change {
                        callback.run(None);
                    }
                }
                _ => {}
            }
        }
    }) as Box<dyn Fn(_)>);

    KEYDOWN_LISTENER_ADDED.call_once(|| {
        if let Some(w) = web_sys::window() {
            let _ = w.add_event_listener_with_callback(
                "keydown",
                keydown_closure.as_ref().unchecked_ref(),
            );
        }
    });
    keydown_closure.forget();

    // Handle trigger node execution
    let handle_trigger = move |node_id: u32| {
        use crate::components::execution_engine::{
            get_upstream_nodes, Task, TaskStatus, Timestamp, TraceLevel,
        };

        let nodes_snapshot = nodes.get();
        let connections_snapshot = connections.get();

        if nodes_snapshot
            .iter()
            .find(|n| n.id == node_id && n.node_type == "trigger")
            .is_none()
        {
            return;
        }

        let exec_order_ids =
            execute_downstream_order(&nodes_snapshot, &connections_snapshot, node_id);

        let get_json_str = |json: &str, key: &str| -> String {
            let pattern = format!(r#""{}":"#, key);
            json.find(&pattern)
                .map(|start| {
                    let value_start = start + pattern.len();
                    let rest = &json[value_start..];
                    if rest.starts_with('"') {
                        let end = rest[1..]
                            .find('"')
                            .map(|i| i + 1)
                            .unwrap_or(rest.len());
                        rest[1..end].to_string()
                    } else {
                        rest.split(',')
                            .next()
                            .unwrap_or("")
                            .split('}')
                            .next()
                            .unwrap_or("")
                            .to_string()
                    }
                })
                .unwrap_or_default()
        };

        spawn_local(async move {
            let mut exec = ExecutionState::new();
            exec.running = true;

            let mut node_results: HashMap<u32, String> = HashMap::new();

            for exec_node_id in exec_order_ids.iter().copied() {
                let upstream_ids = get_upstream_nodes(&connections_snapshot, exec_node_id);
                let upstream: HashMap<u32, String> = upstream_ids
                    .into_iter()
                    .filter_map(|id| node_results.get(&id).map(|r| (id, r.clone())))
                    .collect();

                if exec_node_id == node_id {
                    let mut task = Task::new(exec_node_id, "trigger", None);
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Timestamp::now());
                    task.add_message(
                        &format!("Trigger fired [task_id={}]", task.id),
                        TraceLevel::Info,
                    );
                    task.finished_at = Some(Timestamp::now());
                    task.status = TaskStatus::Complete;
                    exec.tasks.push(task);
                } else if let Some(node) = nodes_snapshot.iter().find(|n| n.id == exec_node_id) {
                    let parent_id = exec.tasks.last().map(|t| t.id.clone());

                    if node.node_type == "model" {
                        let config_json = connections_snapshot
                            .iter()
                            .find(|c| c.target_node_id == exec_node_id && c.target_port_name == "config")
                            .and_then(|c| node_results.get(&c.source_node_id))
                            .cloned()
                            .unwrap_or_else(|| {
                                r#"{"format":"openai","model_name":"","api_key":"","custom_url":""}"#.to_string()
                            });

                        let prompt_text = connections_snapshot
                            .iter()
                            .find(|c| c.target_node_id == exec_node_id && c.target_port_name == "prompt")
                            .and_then(|c| node_results.get(&c.source_node_id))
                            .cloned()
                            .unwrap_or_default();

                        let config = crate::components::canvas::state::ModelConfig {
                            format: get_json_str(&config_json, "format"),
                            model_name: get_json_str(&config_json, "model_name"),
                            api_key: get_json_str(&config_json, "api_key"),
                            custom_url: get_json_str(&config_json, "custom_url"),
                        };

                        let temperature = 1.0;

                        let mut model_task = Task::new(exec_node_id, "model", parent_id.clone());
                        model_task.status = TaskStatus::Running;
                        model_task.started_at = Some(Timestamp::now());
                        model_task.add_message(
                            &format!(
                                "Model call: {} / {} / prompt_len={}",
                                config.format, config.model_name, prompt_text.len()
                            ),
                            TraceLevel::Info,
                        );

                        let result = call_llm_complete(
                            config.format.clone(),
                            config.model_name.clone(),
                            config.api_key.clone(),
                            config.custom_url.clone(),
                            prompt_text.clone(),
                            temperature,
                        )
                        .await;

                        match result {
                            Ok(output) => {
                                let status = if output.error.is_empty() {
                                    TaskStatus::Complete
                                } else {
                                    TaskStatus::Error
                                };
                                let trace_msg = if output.error.is_empty() {
                                    format!(
                                        "LLM result: {} (model={} finish_reason={} tokens={})",
                                        output.text, output.model, output.finish_reason, output.tokens_used
                                    )
                                } else {
                                    format!("LLM error: {}", output.error)
                                };
                                model_task.status = status;
                                model_task.finished_at = Some(Timestamp::now());
                                model_task.result = Some(output.text.clone());
                                model_task.add_message(&trace_msg, TraceLevel::Info);
                                node_results.insert(exec_node_id, output.text);
                            }
                            Err(e) => {
                                model_task.status = TaskStatus::Error;
                                model_task.finished_at = Some(Timestamp::now());
                                model_task.add_message(
                                    &format!("Model call failed: {}", e),
                                    TraceLevel::Error,
                                );
                            }
                        }
                        exec.tasks.push(model_task);
                    } else if node.node_type == "retrieval" {
                        let retrieval_variant = node.variant.clone();
                        let query_text = connections_snapshot
                            .iter()
                            .find(|c| c.target_node_id == exec_node_id && c.target_port_name == "query")
                            .and_then(|c| node_results.get(&c.source_node_id))
                            .cloned()
                            .unwrap_or_default();

                        let (collection, output_type) = if let NodeVariant::Retrieval { query: _, output, collection } = retrieval_variant {
                            (collection, output)
                        } else {
                            (String::new(), "TextOnly".to_string())
                        };

                        let top_k = 4usize;
                        let group_id = "".to_string();
                        let api_key = "".to_string();

                        let mut task = Task::new(exec_node_id, "retrieval", parent_id.clone());
                        task.status = TaskStatus::Running;
                        task.started_at = Some(Timestamp::now());
                        task.add_message(
                            &format!("Retrieving from '{}': {}", collection, query_text),
                            TraceLevel::Info,
                        );

                        let result = call_retrieve(
                            collection.clone(),
                            query_text.clone(),
                            top_k,
                            group_id,
                            api_key,
                            output_type.clone(),
                        )
                        .await;

                        match result {
                            Ok(results) => {
                                let output_text = results
                                    .iter()
                                    .map(|r| r.text.clone())
                                    .collect::<Vec<_>>()
                                    .join("\n---\n");
                                task.status = TaskStatus::Complete;
                                task.finished_at = Some(Timestamp::now());
                                task.result = Some(output_text.clone());
                                task.add_message(
                                    &format!("Retrieved {} results from '{}'", results.len(), collection),
                                    TraceLevel::Info,
                                );
                                node_results.insert(exec_node_id, output_text);
                            }
                            Err(e) => {
                                task.status = TaskStatus::Error;
                                task.finished_at = Some(Timestamp::now());
                                task.add_message(&format!("Retrieval error: {}", e), TraceLevel::Error);
                            }
                        }
                        exec.tasks.push(task);
                    } else {
                        let (mut task, result) = execute_node_sync(node, &upstream, parent_id);
                        if task.messages.len() == 1 {
                            task.messages[0].message = format!(
                                "{} [node_id={}, parent_id={:?}]",
                                node.label, task.node_id, task.parent_id
                            );
                        }

                        if node.node_type == "text_output" {
                            if let Some(ref input) = result {
                                set_nodes.update(|nodes: &mut Vec<NodeState>| {
                                    if let Some(n) = nodes.iter_mut().find(|n| n.id == exec_node_id) {
                                        if let crate::components::canvas::state::NodeVariant::ChatOutput { response } = &mut n.variant {
                                            *response = input.clone();
                                        }
                                    }
                                });
                            }
                        }

                        if let Some(ref r) = result {
                            node_results.insert(exec_node_id, r.clone());
                        }
                        exec.tasks.push(task);
                    }
                }
            }

            exec.running = false;
            set_execution_state.set(exec);
        });
    };

    // Callback to start palette drag

    let on_palette_drag_start: Callback<String> = Callback::new(move |node_type: String| {
        set_dragging_node_type.set(Some(node_type));
    });

    // Callbacks to start graph panel drag
    let on_bundle_drag_start: Callback<String> = Callback::new(move |bundle_id: String| {
        let bundle = BUNDLED_GROUPS.iter().find(|b| b.id == bundle_id);
        let label = bundle.map(|b| b.name.to_string()).unwrap_or_else(|| bundle_id.clone());
        let (w, h) = bundle.map(|b| graph_bounds(&b.nodes)).unwrap_or((160.0, 100.0));
        set_dragging_graph_label.set(Some(label));
        set_drag_preview_w.set(w);
        set_drag_preview_h.set(h);
    });

    let on_selection_drag_start: Callback<String> = Callback::new(move |selection_id: String| {
        let selections = saved_selections.get();
        let selection = selections.iter().find(|s| s.id == selection_id);
        let label = selection.map(|s| s.name.clone()).unwrap_or_else(|| "Selection".to_string());
        let (w, h) = selection.map(|s| graph_bounds(&s.nodes)).unwrap_or((160.0, 100.0));
        set_dragging_graph_label.set(Some(label));
        set_drag_preview_w.set(w);
        set_drag_preview_h.set(h);
    });

    // Handle node drop from palette
    let handle_node_drop = move |node_type: String, x: f64, y: f64| {
        let node_id = next_node_id.get();
        let label = NODE_TYPES
            .iter()
            .find(|n| n.id == node_type)
            .map(|n| n.name)
            .unwrap_or(&node_type)
            .to_string();

        let new_node = NodeState {
            id: node_id,
            x: x - 75.0,
            y: y - 50.0,
            node_type: node_type.clone(),
            label,
            selected: false,
            status: NodeStatus::Pending,
            variant: default_variant_for_type(&node_type),
            ports: default_ports_for_type(&node_type),
        };

        set_nodes.update(|nodes: &mut Vec<NodeState>| nodes.push(new_node));
        set_next_node_id.update(|n| *n += 1);
        set_selected_node_ids.update(|ids| {
            ids.clear();
            ids.insert(node_id);
        });
    };

    let handle_left_divider_mouse_down = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        set_dragging_left.set(true);
    };

    let handle_right_divider_mouse_down = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        set_dragging_right.set(true);
    };

    // Handle node inspection from right-click
    let handle_node_inspect = {
        let set_inspector_tabs = set_inspector_tabs.clone();
        let set_active_inspector_tab = set_active_inspector_tab.clone();
        let inspector_tabs = inspector_tabs.clone();
        let active_inspector_tab = active_inspector_tab.clone();

        move |(node_id, is_double_click): (u32, bool)| {
            let tabs = inspector_tabs.get();

            // Handle double-click: pin the pending preview tab
            if is_double_click {
                if let Some(idx) = tabs
                    .iter()
                    .position(|t| t.node_id == node_id && t.is_preview)
                {
                    set_inspector_tabs.update(|tabs| {
                        if let Some(tab) = tabs.get_mut(idx) {
                            tab.is_preview = false;
                        }
                    });
                }
                return;
            }

            // Check if node already has a tab
            if let Some(existing_idx) = tabs.iter().position(|t| t.node_id == node_id) {
                // Switch to existing tab
                set_active_inspector_tab.set(Some(existing_idx));
                return;
            }

            // Check if we should replace the current preview tab
            let should_replace_preview = active_inspector_tab
                .get()
                .map_or(false, |idx| tabs.get(idx).map_or(false, |t| t.is_preview));

            if should_replace_preview {
                // Replace current preview tab
                let active_idx = active_inspector_tab.get().unwrap();
                set_inspector_tabs.update(|tabs| {
                    if let Some(tab) = tabs.get_mut(active_idx) {
                        tab.node_id = node_id;
                        tab.is_preview = true;
                    }
                });
            } else {
                // Add new preview tab
                let new_tab = InspectorTab {
                    node_id,
                    is_preview: true,
                };
                let new_idx = tabs.len();
                set_inspector_tabs.update(|tabs| {
                    tabs.push(new_tab);
                });
                set_active_inspector_tab.set(Some(new_idx));
            }
        }
    };

    // Handle node update from inspector
    let handle_update_node = {
        let set_nodes = set_nodes.clone();
        move |(node_id, new_variant): (u32, crate::components::canvas::state::NodeVariant)| {
            set_nodes.update(|nodes| {
                if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                    node.variant = new_variant;
                }
            });
        }
    };

    let handle_inspector_divider_mouse_down = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        set_inspector_dragging.set(true);
    };

    let handle_mouse_up = move |_ev: web_sys::MouseEvent| {
        set_dragging_left.set(false);
        set_dragging_right.set(false);
        set_inspector_dragging.set(false);
        // Clear drag preview state
        set_dragging_node_type.set(None);
        set_dragging_graph_label.set(None);
        set_drag_preview_w.set(0.0);
        set_drag_preview_h.set(0.0);
        // Clear window drag properties to prevent stale state on subsequent clicks
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::delete_property(&window, &"draggedNodeType".into());
            let _ = js_sys::Reflect::delete_property(&window, &"draggedBundleId".into());
            let _ = js_sys::Reflect::delete_property(&window, &"draggedSelectionId".into());
        }
    };

    // Global mouse move for drag preview
    let handle_global_mousemove = move |ev: web_sys::MouseEvent| {
        if dragging_node_type.get().is_some() || dragging_graph_label.get().is_some() {
            set_drag_x.set(ev.client_x() as f64);
            set_drag_y.set(ev.client_y() as f64);
        }
        if dragging_left.get() {
            let new_width = ev.client_x();
            if new_width >= 180 && new_width <= 500 {
                set_left_width.set(new_width);
            }
        }
        if dragging_right.get() {
            let window = web_sys::window().unwrap();
            let inner_width = window.inner_width().unwrap().as_f64().unwrap() as i32;
            let new_width = inner_width - ev.client_x();
            if new_width >= 180 && new_width <= 500 {
                set_right_width.set(new_width);
            }
        }
        if inspector_dragging.get() {
            let window = web_sys::window().unwrap();
            let inner_height = window.inner_height().unwrap().as_f64().unwrap() as i32;
            let new_height = inner_height - ev.client_y();
            if new_height >= 150 && new_height <= 500 {
                set_inspector_height.set(new_height);
            }
        }
    };

    // Callback to load a saved selection into canvas
    let on_load_selection = {
        let next_node_id = next_node_id.clone();
        let next_connection_id = next_connection_id.clone();
        let apply_load_result = apply_load_result.clone();
        let load_offset_counter = load_offset_counter.clone();
        let set_load_offset_counter = set_load_offset_counter.clone();
        Callback::new(move |selection: SavedSelection| {
            let (mut new_nodes, new_conns, next_id, next_conn, new_ids) =
                crate::components::save_load::load_selection(
                    selection,
                    next_node_id.get(),
                    next_connection_id.get(),
                );
            let c = load_offset_counter.get();
            shift_nodes(&mut new_nodes, c as f64 * 20.0, c as f64 * 20.0);
            set_load_offset_counter.set(c + 1);
            apply_load_result((new_nodes, new_conns, next_id, next_conn, new_ids), "Selection loaded");
        })
    };

    // Callback to load a bundled group into canvas
    let on_load_bundle = {
        let next_node_id = next_node_id.clone();
        let next_connection_id = next_connection_id.clone();
        let apply_load_result = apply_load_result.clone();
        let load_offset_counter = load_offset_counter.clone();
        let set_load_offset_counter = set_load_offset_counter.clone();
        let llm_defaults = llm_defaults.clone();
        Callback::new(move |mut bundle: BundledGroup| {
            apply_llm_defaults_to_bundle(&mut bundle, &llm_defaults.get_value());
            let selection = bundle_to_selection(bundle);
            let (mut new_nodes, new_conns, next_id, next_conn, new_ids) =
                crate::components::save_load::load_selection(
                    selection,
                    next_node_id.get(),
                    next_connection_id.get(),
                );
            let c = load_offset_counter.get();
            shift_nodes(&mut new_nodes, c as f64 * 20.0, c as f64 * 20.0);
            set_load_offset_counter.set(c + 1);
            apply_load_result((new_nodes, new_conns, next_id, next_conn, new_ids), "Bundle loaded");
        })
    };

    // Callback when a bundled group is dropped on the canvas
    let on_bundle_drop = {
        let next_node_id = next_node_id.clone();
        let next_connection_id = next_connection_id.clone();
        let apply_load_result = apply_load_result.clone();
        let llm_defaults = llm_defaults.clone();
        Callback::new(move |(bundle_id, x, y): (String, f64, f64)| {
            let Some(mut bundle) = BUNDLED_GROUPS.iter().find(|b| b.id == bundle_id).cloned() else { return };
            apply_llm_defaults_to_bundle(&mut bundle, &llm_defaults.get_value());
            let selection = bundle_to_selection(bundle);
            let (mut new_nodes, new_conns, next_id, next_conn, new_ids) =
                crate::components::save_load::load_selection(
                    selection,
                    next_node_id.get(),
                    next_connection_id.get(),
                );
            center_nodes_at(&mut new_nodes, x, y);
            apply_load_result((new_nodes, new_conns, next_id, next_conn, new_ids), "Bundle loaded");
        })
    };

    // Callback when a saved selection is dropped on the canvas
    let on_selection_drop = {
        let next_node_id = next_node_id.clone();
        let next_connection_id = next_connection_id.clone();
        let saved_selections = saved_selections.clone();
        let apply_load_result = apply_load_result.clone();
        Callback::new(move |(selection_id, x, y): (String, f64, f64)| {
            let selections = saved_selections.get();
            let Some(selection) = selections.iter().find(|s| s.id == selection_id).cloned() else { return };
            let (mut new_nodes, new_conns, next_id, next_conn, new_ids) =
                crate::components::save_load::load_selection(
                    selection,
                    next_node_id.get(),
                    next_connection_id.get(),
                );
            center_nodes_at(&mut new_nodes, x, y);
            apply_load_result((new_nodes, new_conns, next_id, next_conn, new_ids), "Selection loaded");
        })
    };

    // Callback to delete a saved selection
    let on_delete_selection = {
        let saved_selections_clone = saved_selections.clone();
        let set_saved_selections = set_saved_selections.clone();
        let add_toast = add_toast.clone();
        Callback::new(move |id: String| {
            set_saved_selections.update(|selections| {
                selections.retain(|s| s.id != id);
            });
            let selections = saved_selections_clone.get();
            crate::components::save_load::save_saved_selections_to_storage(&selections);
            add_toast("Selection deleted".to_string(), ToastType::Info);
        })
    };

    view! {
        <div
            class="app-layout"
            on:mousemove={handle_global_mousemove}
            on:mouseup={handle_mouse_up}
            on:mouseleave={handle_mouse_up}
        >
            <div class="app-layout-main">
                {/* Left Panel */}
                <div
                    class="panel"
                    style:width=move || format!("{}px", left_width.get())
                >
                    <LeftPanel
                        on_drag_start={Some(on_palette_drag_start)}
                        saved_selections={saved_selections.into()}
                        on_load_selection={on_load_selection}
                        on_delete_selection={on_delete_selection}
                        on_load_bundle={on_load_bundle}
                        on_bundle_drag_start={Some(on_bundle_drag_start)}
                        on_selection_drag_start={Some(on_selection_drag_start)}
                    />
                </div>

                {/* Left Divider */}
                <div
                    class="divider"
                    on:mousedown={handle_left_divider_mouse_down}
                ></div>

                {/* Canvas + Inspector column */}
                <div class="canvas-column">
                    <Canvas
                        nodes={nodes.into()}
                        connections={connections.into()}
                        selected_node_ids={selected_node_ids.into()}
                        set_selected_node_ids={set_selected_node_ids}
                        set_nodes={set_nodes}
                        set_connections={set_connections}
                        deleting_node_id={Some(deleting_node_id.into())}
                        on_node_drop={Some(Callback::from(handle_node_drop))}
                        on_bundle_drop={Some(on_bundle_drop)}
                        on_selection_drop={Some(on_selection_drop)}
                        zoom={zoom.into()}
                        set_zoom={set_zoom}
                        left_width={Some(left_width.into())}
                        right_width={Some(right_width.into())}
                        inspector_height={Some(inspector_height.into())}
                        on_trigger={Some(Callback::new(handle_trigger))}
                        on_text_change={Some(Callback::new(move |(node_id, new_text)| handle_text_change(node_id, new_text)))}
                        on_node_right_click={Some(Callback::new(handle_node_inspect))}
                        on_interaction_start={Some(Callback::new(move |_| begin_undo_suppression()))}
                        on_interaction_end={Some(Callback::new(move |_| end_undo_suppression()))}
                    />

                    <div
                        class="divider divider-horizontal"
                        on:mousedown={handle_inspector_divider_mouse_down}
                    ></div>

                    {/* Inspector panel */}
                    <InspectorPanel
                        tabs={inspector_tabs.into()}
                        active_tab={active_inspector_tab.into()}
                        nodes={nodes.into()}
                        height={inspector_height.into()}
                        set_active_tab={Callback::new(move |idx| set_active_inspector_tab.set(idx))}
                        set_tabs={Callback::new(move |tabs| set_inspector_tabs.set(tabs))}
                        on_update_node={Callback::new(handle_update_node)}
                    />
                </div>

                {/* Right Divider */}
                <div
                    class="divider"
                    on:mousedown={handle_right_divider_mouse_down}
                ></div>

                {/* Right Panel */}
                <div
                    class="panel panel-right"
                    style:width=move || format!("{}px", right_width.get())
                >
                    <RightPanel execution={execution_state.into()} />
                </div>
            </div>

            {/* Drag Preview */}
            {move || {
                if let Some(node_type) = dragging_node_type.get() {
                    let label = NODE_TYPES
                        .iter()
                        .find(|n| n.id == node_type)
                        .map(|n| n.name.to_string())
                        .unwrap_or_else(|| node_type.clone());
                    Some(view! {
                        <div
                            class="drag-preview"
                            style:left={format!("{}px", drag_x.get())}
                            style:top={format!("{}px", drag_y.get())}
                        >
                            {label}
                        </div>
                    }.into_any())
                } else if let Some(label) = dragging_graph_label.get() {
                    Some(view! {
                        <div
                            class="drag-preview drag-preview-graph"
                            style:left={format!("{}px", drag_x.get())}
                            style:top={format!("{}px", drag_y.get())}
                            style:width={format!("{}px", drag_preview_w.get() * zoom.get())}
                            style:height={format!("{}px", drag_preview_h.get() * zoom.get())}
                        >
                            {label}
                        </div>
                    }.into_any())
                } else {
                    None
                }
            }}

            {/* Toast notifications */}
            <ToastContainer toasts={toasts.into()} on_dismiss={Callback::new(move |id| {
                set_toasts.update(|t| t.retain(|toast| toast.id != id));
            })} />
        </div>
    }
}
