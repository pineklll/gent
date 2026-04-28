use crate::components::canvas::state::{NodeVariant, PortDirection, PortType, PortWithOffset, QueryTranslationMode};
use leptos::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// Renders variant-specific body content for a node
fn render_variant_body(
    variant: &NodeVariant,
    node_id: u32,
    on_text_change: &Option<Callback<(u32, String)>>,
    on_limit_change: &Option<Callback<(u32, usize)>>,
    on_mode_change: &Option<Callback<(u32, QueryTranslationMode)>>,
) -> impl IntoView {
    match variant {
        NodeVariant::UserInput { text } => {
            let cb = on_text_change.clone();
            view! {
                <input
                    type="text"
                    class="node-variant-input"
                    value={text.clone()}
                    placeholder="Enter text..."
                    on:change={move |ev| {
                        let new_text = event_target_value(&ev);
                        if let Some(callback) = cb {
                            callback.run((node_id, new_text));
                        }
                    }}
                />
            }.into_any()
        }
        NodeVariant::FileInput { path } => view! {
            <input
                type="text"
                class="node-variant-input"
                value={path.clone()}
                placeholder="File path..."
            />
        }.into_any(),
        NodeVariant::Template { template } => view! {
            <textarea
                class="node-variant-textarea"
                placeholder="Template..."
                rows="3"
            >{template.clone()}</textarea>
        }.into_any(),
        NodeVariant::Retrieval { virtual_uri, limit } => {
            let cb = on_text_change.clone();
            let limit_cb = on_limit_change.clone();
            view! {
                <div class="node-variant-fields">
                    <input
                        type="text"
                        class="node-variant-input"
                        value={virtual_uri.clone()}
                        placeholder="viking://resources/..."
                        on:change={move |ev| {
                            let new_value = event_target_value(&ev);
                            if let Some(callback) = cb {
                                callback.run((node_id, new_value));
                            }
                        }}
                    />
                    <div class="node-variant-field">
                        <label>"limit"</label>
                        <input
                            type="number"
                            class="node-variant-input small"
                            value={*limit as f64}
                            min="1"
                            max="100"
                            on:change={move |ev| {
                                if let Ok(new_value) = event_target_value(&ev).parse::<usize>() {
                                    if let Some(callback) = limit_cb {
                                        callback.run((node_id, new_value));
                                    }
                                }
                            }}
                        />
                    </div>
                </div>
            }.into_any()
        }
        NodeVariant::Index { collection } => view! {
            <input
                type="text"
                class="node-variant-input"
                value={collection.clone()}
                placeholder="collection name"
            />
        }.into_any(),
        NodeVariant::Summarizer { max_length } => view! {
            <div class="node-variant-field">
                <label>"Max Length"</label>
                <input
                    type="number"
                    class="node-variant-input"
                    value={*max_length as f64}
                    min="50"
                    max="2000"
                />
            </div>
        }.into_any(),
        NodeVariant::PlannerAgent { goal } => view! {
            <textarea
                class="node-variant-textarea"
                placeholder="Agent goal..."
                rows="2"
            >{goal.clone()}</textarea>
        }.into_any(),
        NodeVariant::ExecutorAgent { task } => view! {
            <textarea
                class="node-variant-textarea"
                placeholder="Task description..."
                rows="2"
            >{task.clone()}</textarea>
        }.into_any(),
        NodeVariant::WebSearch { query, num_results } => view! {
            <div class="node-variant-fields">
                <input
                    type="text"
                    class="node-variant-input"
                    value={query.clone()}
                    placeholder="Search query..."
                />
                <div class="node-variant-field">
                    <label>"Results"</label>
                    <input
                        type="number"
                        class="node-variant-input small"
                        value={*num_results as f64}
                        min="1"
                        max="20"
                    />
                </div>
            </div>
        }.into_any(),
        NodeVariant::CodeExecute { code, language } => view! {
            <div class="node-variant-fields">
                <textarea
                    class="node-variant-textarea code"
                    placeholder="Code..."
                    rows="2"
                >{code.clone()}</textarea>
                <input
                    type="text"
                    class="node-variant-input small"
                    value={language.clone()}
                    placeholder="Language..."
                />
            </div>
        }.into_any(),
        NodeVariant::IfCondition { branches } => view! {
            <div class="node-variant-field">
                <label>"Branches"</label>
                <input
                    type="number"
                    class="node-variant-input"
                    value={*branches as f64}
                    min="2"
                    max="10"
                />
            </div>
        }.into_any(),
        NodeVariant::Loop { iterations } => view! {
            <div class="node-variant-field">
                <label>"Iterations"</label>
                <input
                    type="number"
                    class="node-variant-input"
                    value={*iterations as f64}
                    min="1"
                    max="100"
                />
            </div>
        }.into_any(),
        NodeVariant::ChatOutput { response } => view! {
            <textarea
                class="node-variant-textarea"
                placeholder="Response..."
                rows="2"
            >{response.clone()}</textarea>
        }.into_any(),
        NodeVariant::JsonOutput { schema } => view! {
            <textarea
                class="node-variant-textarea code"
                placeholder="JSON Schema..."
                rows="2"
            >{schema.clone()}</textarea>
        }.into_any(),
        NodeVariant::ModelConfig { format, model_name, api_key, custom_url } => view! {
            <div class="node-variant-fields">
                <div class="node-variant-field">
                    <label>"Format"</label>
                    <select class="node-variant-select">
                        <option value="openai" selected={format == "openai"}>"OpenAI"</option>
                        <option value="anthropic" selected={format == "anthropic"}>"Anthropic"</option>
                    </select>
                </div>
                <div class="node-variant-field">
                    <label>"Model Name"</label>
                    <input
                        type="text"
                        class="node-variant-input"
                        value={model_name.clone()}
                        placeholder="gpt-4o-mini"
                    />
                </div>
                <div class="node-variant-field">
                    <label>"API Key"</label>
                    <input
                        type="password"
                        class="node-variant-input"
                        value={api_key.clone()}
                        placeholder="key or leave empty for env"
                    />
                </div>
                <div class="node-variant-field">
                    <label>"API Endpoint"</label>
                    <input
                        type="text"
                        class="node-variant-input"
                        value={custom_url.clone()}
                        placeholder="http://localhost:11434/v1"
                    />
                </div>
            </div>
        }.into_any(),
        NodeVariant::Model => view! {
            <div class="node-variant-fields" />
        }.into_any(),
        NodeVariant::QueryTranslation { mode, .. } => {
            let mode_cb = on_mode_change.clone();
            let current_mode = match mode {
                QueryTranslationMode::MultiQuery { .. } => "multi_query",
                QueryTranslationMode::StepBack => "step_back",
                QueryTranslationMode::RagFusion { .. } => "rag_fusion",
            };
            view! {
                <div class="node-variant-fields">
                    <div class="node-variant-field">
                        <label>"Mode"</label>
                        <select
                            class="node-variant-select"
                            on:change={move |ev| {
                                let new_mode = event_target_value(&ev);
                                let new_query_mode = match new_mode.as_str() {
                                    "multi_query" => QueryTranslationMode::MultiQuery { num_queries: 3 },
                                    "step_back" => QueryTranslationMode::StepBack,
                                    "rag_fusion" => QueryTranslationMode::RagFusion { num_queries: 3, k: 60, limit_per_query: 10 },
                                    _ => return,
                                };
                                if let Some(cb) = &mode_cb {
                                    cb.run((node_id, new_query_mode));
                                }
                            }}
                        >
                            <option value="multi_query" selected={current_mode == "multi_query"}>"Multi-Query"</option>
                            <option value="step_back" selected={current_mode == "step_back"}>"Step-Back"</option>
                            <option value="rag_fusion" selected={current_mode == "rag_fusion"}>"RAG-Fusion"</option>
                        </select>
                    </div>
                </div>
            }.into_any()
        }
        NodeVariant::ReActLoop { action_type, max_iterations } => {
            let action_type_cb = on_text_change.clone();
            let max_iter_cb = on_limit_change.clone();
            view! {
                <div class="node-variant-fields">
                    <div class="node-variant-field">
                        <label>"Action"</label>
                        <select class="node-variant-select"
                            on:change={move |ev| {
                                let new_action = event_target_value(&ev);
                                if let Some(callback) = &action_type_cb {
                                    callback.run((node_id, new_action));
                                }
                            }}
                        >
                            <option value="retrieval" selected={action_type == "retrieval"}>"Retrieval"</option>
                            <option value="web_search" selected={action_type == "web_search"} disabled>"Web Search (future)"</option>
                            <option value="code_exec" selected={action_type == "code_exec"} disabled>"Code Exec (future)"</option>
                        </select>
                    </div>
                    <div class="node-variant-field">
                        <label>"Max Iterations"</label>
                        <input
                            type="number"
                            class="node-variant-input small"
                            value={*max_iterations as f64}
                            min="1"
                            max="20"
                            on:change={move |ev| {
                                if let Ok(new_value) = event_target_value(&ev).parse::<usize>() {
                                    if let Some(callback) = &max_iter_cb {
                                        callback.run((node_id, new_value));
                                    }
                                }
                            }}
                        />
                    </div>
                </div>
            }.into_any()
        }
        // Trigger variant is handled separately in the GraphNode view
        _ => view! { <div /> }.into_any(),
    }
}

/// A DOM-based graph node component
#[component]
pub fn GraphNode(
    x: f64,
    y: f64,
    label: String,
    selected: bool,
    node_id: u32,
    variant: NodeVariant,
    ports: Vec<PortWithOffset>,
    has_input_connection: bool,
    #[prop(default = false)] is_deleting: bool,
    on_output_drag_start: Option<Callback<(u32, String, f64, f64)>>,
    on_input_drag_end: Option<Callback<(u32, String, f64, f64)>>,
    on_input_click: Option<Callback<(u32,)>>,
    on_input_reroute_start: Option<Callback<(u32,)>>,
    cancel_connection_drag: Option<Callback<(), ()>>,
    on_trigger: Option<Callback<u32>>,
    #[prop(default = None)] _on_variant_change: Option<Callback<NodeVariant>>,
    #[prop(default = None)] on_text_change: Option<Callback<(u32, String)>>,
    /// Callback when limit changes in a Retrieval node
    #[prop(default = None)] on_limit_change: Option<Callback<(u32, usize)>>,
    /// Callback when query translation mode changes
    #[prop(default = None)] on_mode_change: Option<Callback<(u32, QueryTranslationMode)>>,
    /// Callback when node is right-clicked for inspection
    /// Args: (node_id, is_double_click)
    #[prop(default = None)]
    on_node_right_click: Option<Callback<(u32, bool)>>,
) -> impl IntoView {
    let class = if selected { "node selected" } else { "node" };
    let class = if is_deleting {
        format!("{} deleting", class)
    } else {
        class.to_string()
    };

    // Right-click double-click detection state (local to this node instance)
    let last_right_click = Rc::new(RefCell::new(None::<f64>));

    // Calculate content offset based on number of ports
    // Ports are at: 50px, 75px, 100px, 125px... (FIRST_PORT_OFFSET + i * PORT_SPACING)
    // Content should start below the last port's bottom edge plus a buffer
    let in_count = ports
        .iter()
        .filter(|p| p.port.direction == PortDirection::In)
        .count();
    let out_count = ports
        .iter()
        .filter(|p| p.port.direction == PortDirection::Out)
        .count();
    let max_ports = in_count.max(out_count);
    let content_offset = (max_ports + 1) as f64 * 25.0;

    // Track if mouse moved between mousedown and mouseup on input port
    // Also track if this input has an existing connection (for reroute detection)
    let input_drag_start = std::rc::Rc::new(std::cell::Cell::new(Option::<(f64, f64, bool)>::None));

    // Output port mousedown - start connection drag
    // NO stop_propagation - let events bubble to canvas for reliable handling
    let handle_output_mousedown = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        // Extract port_name from the DOM element's data attribute
        let port_name = ev
            .target()
            .and_then(|t| {
                let element: Result<web_sys::Element, _> = t.dyn_into();
                element.ok()
            })
            .and_then(|el: web_sys::Element| el.get_attribute("data-port-name"))
            .unwrap_or_else(|| "output".to_string());
        if let Some(cb) = &on_output_drag_start {
            cb.run((
                node_id,
                port_name,
                ev.client_x() as f64,
                ev.client_y() as f64,
            ));
        }
    };

    view! {
        <div
            class={class}
            data-node-id={node_id}
            style:left={format!("{}px", x)}
            style:top={format!("{}px", y)}
            on:contextmenu={{
                let last_click = last_right_click.clone();
                let on_node_right_click = on_node_right_click.clone();
                move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    let now = js_sys::Date::now();
                    let is_double = last_click.borrow().map_or(false, |t| now - t < 300.0);
                    *last_click.borrow_mut() = Some(now);
                    if let Some(cb) = &on_node_right_click {
                        cb.run((node_id, is_double));
                    }
                }
            }}
        >
            <div class="node-header">
                <span>{label}</span>
            </div>
            <div class="node-body" style:padding-top={format!("{}px", content_offset)}>
                {match variant {
                    NodeVariant::Trigger => view! {
                        <button
                            class="trigger-btn"
                            on:mousedown={move |ev| {
                                ev.prevent_default();
                                if let Some(cb) = &on_trigger {
                                    cb.run(node_id);
                                }
                            }}
                        >
                            "Run"
                        </button>
                    }.into_any(),
                    _ => render_variant_body(&variant, node_id, &on_text_change, &on_limit_change, &on_mode_change).into_any(),
                }}
            </div>
            {/* Dynamic input ports */}
            {ports.iter().filter(|p| p.port.direction == PortDirection::In).map(|port| {
                let port_class_str = match &port.port.port_type {
                    PortType::Text => "node-port text input".to_string(),
                    PortType::Image => "node-port image input".to_string(),
                    PortType::Audio => "node-port audio input".to_string(),
                    PortType::File => "node-port file input".to_string(),
                    PortType::Embeddings => "node-port embeddings input".to_string(),
                    PortType::Trigger => "node-port trigger input".to_string(),
                };
                let port_name = port.port.name.clone();
                let top_offset = port.top_offset;
                let input_drag_start_clone = input_drag_start.clone();
                let on_input_reroute_start_clone = on_input_reroute_start.clone();
                let handle_input_mousedown = move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    input_drag_start_clone.set(Some((ev.client_x() as f64, ev.client_y() as f64, has_input_connection)));
                    if has_input_connection {
                        if let Some(cb) = &on_input_reroute_start_clone {
                            cb.run((node_id,));
                        }
                    }
                };
                let input_drag_start_clone2 = input_drag_start.clone();
                let on_input_drag_end_clone = on_input_drag_end.clone();
                let on_input_click_clone = on_input_click.clone();
                let cancel_connection_drag_clone = cancel_connection_drag.clone();
                let port_name_for_drag_end = port_name.clone();
                let handle_input_mouseup = move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    let start_pos = input_drag_start_clone2.get();
                    input_drag_start_clone2.set(None);
                    let was_click = start_pos.map(|(sx, sy, _)| {
                        let dx = (ev.client_x() as f64 - sx).abs();
                        let dy = (ev.client_y() as f64 - sy).abs();
                        dx < 5.0 && dy < 5.0
                    }).unwrap_or(false);
                    if was_click && start_pos.is_some() {
                        if let Some(cb) = &on_input_click_clone {
                            cb.run((node_id,));
                        }
                        if let Some(cb) = &cancel_connection_drag_clone {
                            cb.run(());
                        }
                    } else {
                        if let Some(cb) = &on_input_drag_end_clone {
                            cb.run((node_id, port_name_for_drag_end.clone(), ev.client_x() as f64, ev.client_y() as f64));
                        }
                    }
                };
                let port_name_clone = port_name.clone();
                let port_name_clone2 = port_name.clone();
                view! {
                    <div
                        class={port_class_str}
                        data-port="input"
                        data-node-id={node_id}
                        data-port-name={port_name.clone()}
                        title={port_name_clone}
                        style:top={format!("{}px", top_offset)}
                        on:mousedown=handle_input_mousedown
                        on:mouseup=handle_input_mouseup
                    >
                        <span class="port-label">{port_name_clone2}</span>
                    </div>
                }
            }).collect::<Vec<_>>()}
            {/* Dynamic output ports */}
            {ports.iter().filter(|p| p.port.direction == PortDirection::Out).map(|port| {
                let port_class_str = match &port.port.port_type {
                    PortType::Text => "node-port text output".to_string(),
                    PortType::Image => "node-port image output".to_string(),
                    PortType::Audio => "node-port audio output".to_string(),
                    PortType::File => "node-port file output".to_string(),
                    PortType::Embeddings => "node-port embeddings output".to_string(),
                    PortType::Trigger => "node-port trigger output".to_string(),
                };
                let port_name = port.port.name.clone();
                let top_offset = port.top_offset;
                let port_name_clone = port_name.clone();
                let port_name_clone2 = port_name.clone();
                view! {
                    <div
                        class={port_class_str}
                        data-port="output"
                        data-node-id={node_id}
                        data-port-name={port_name}
                        title={port_name_clone}
                        style:top={format!("{}px", top_offset)}
                        on:mousedown=handle_output_mousedown
                    >
                        <span class="port-label">{port_name_clone2}</span>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
