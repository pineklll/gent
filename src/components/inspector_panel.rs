use crate::components::canvas::state::{NodeState, NodeVariant};
use leptos::prelude::*;

/// A tab in the inspector panel
#[derive(Clone, Debug)]
pub struct InspectorTab {
    pub node_id: u32,
    pub is_preview: bool,
}

/// Bottom inspector panel with tabs
#[component]
pub fn InspectorPanel(
    /// All open tabs
    tabs: Signal<Vec<InspectorTab>>,
    /// Index of active tab (None if no tabs)
    active_tab: Signal<Option<usize>>,
    /// All nodes (to look up node data by ID)
    nodes: Signal<Vec<NodeState>>,
    /// Panel height in pixels
    height: Signal<i32>,
    /// Callback when active tab changes
    set_active_tab: Callback<Option<usize>>,
    /// Callback when tabs list changes (for closing/reordering)
    set_tabs: Callback<Vec<InspectorTab>>,
    /// Callback when a node property is updated
    on_update_node: Callback<(u32, NodeVariant)>,
) -> impl IntoView {
    view! {
        <div class="inspector-panel" style:height=move || format!("{}px", height.get())>
            <div class="inspector-content">
                {move || {
                    if tabs.get().is_empty() {
                        view! {
                            <div class="inspector-empty">
                                "Right-click a node to inspect"
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <TabBar
                                tabs=tabs
                                active_tab=active_tab
                                nodes=nodes
                                set_active_tab=set_active_tab
                                set_tabs=set_tabs
                            />
                            <InspectorContent
                                tabs=tabs
                                active_tab=active_tab
                                nodes=nodes
                                on_update_node=on_update_node
                            />
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[component]
fn TabBar(
    tabs: Signal<Vec<InspectorTab>>,
    active_tab: Signal<Option<usize>>,
    nodes: Signal<Vec<NodeState>>,
    set_active_tab: Callback<Option<usize>>,
    set_tabs: Callback<Vec<InspectorTab>>,
) -> impl IntoView {
    view! {
        <div class="inspector-tab-bar scrollbar-compact">
            {move || {
                let tabs_list = tabs.get();
                let nodes_list = nodes.get();

                tabs_list.iter().enumerate().map(|(idx, tab)| {
                    let is_active = active_tab.get() == Some(idx);
                    let node_id = tab.node_id;

                    // Look up node label
                    let node_label = nodes_list
                        .iter()
                        .find(|n| n.id == node_id)
                        .map(|n| n.label.clone())
                        .unwrap_or_else(|| format!("Node {}", node_id));

                    view! {
                        <div
                            class="inspector-tab"
                            class:active=is_active
                            class:preview=tab.is_preview
                            on:click=move |_| set_active_tab.run(Some(idx))
                        >
                            <span class="tab-label">{node_label}</span>
                            <button
                                class="tab-close"
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    let mut new_tabs = tabs.get();
                                    if idx < new_tabs.len() {
                                        new_tabs.remove(idx);
                                        let active_idx = active_tab.get();
                                        let new_active = if new_tabs.is_empty() {
                                            None
                                        } else if is_active {
                                            Some(idx.saturating_sub(1))
                                        } else if let Some(current_active) = active_idx {
                                            // If closed tab was before active tab, decrement active index
                                            if idx < current_active {
                                                Some(current_active - 1)
                                            } else {
                                                Some(current_active)
                                            }
                                        } else {
                                            None
                                        };
                                        set_tabs.run(new_tabs);
                                        set_active_tab.run(new_active);
                                    }
                                }
                            >
                                "×"
                            </button>
                        </div>
                    }
                }).collect::<Vec<_>>()
            }}
        </div>
    }
}

#[component]
fn InspectorContent(
    tabs: Signal<Vec<InspectorTab>>,
    active_tab: Signal<Option<usize>>,
    nodes: Signal<Vec<NodeState>>,
    on_update_node: Callback<(u32, NodeVariant)>,
) -> impl IntoView {
    view! {
        <div class="inspector-body">
            {move || {
                let tabs = tabs.get();
                let active_idx = active_tab.get()?;
                let tab = tabs.get(active_idx)?;
                let node_id = tab.node_id;
                let nodes = nodes.get();
                let node = nodes.iter().find(|n| n.id == node_id)?.clone();

                Some(view! {
                    <div class="inspector-header">
                        <span class="node-type-badge">{node.node_type.clone()}</span>
                        <span class="node-label">{node.label.clone()}</span>
                    </div>
                    <InspectorProperties node=node on_update_node=on_update_node />
                })
            }}
        </div>
    }
}

#[component]
fn InspectorProperties(
    node: NodeState,
    on_update_node: Callback<(u32, NodeVariant)>,
) -> impl IntoView {
    let node_id = node.id;

    // Render variant-specific property editors
    match node.variant.clone() {
        NodeVariant::UserInput { text } => view! {
            <div class="property-group">
                <label class="property-label">"Text"</label>
                <textarea
                    class="property-textarea"
                    rows="3"
                    prop:value={text.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::UserInput { text: new_value }));
                    }
                >{text.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::FileInput { path } => view! {
            <div class="property-group">
                <label class="property-label">"File Path"</label>
                <input
                    type="text"
                    class="property-input"
                    prop:value={path.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::FileInput { path: new_value }));
                    }
                />
            </div>
        }.into_any(),
        NodeVariant::Trigger => view! {
            <div class="property-group">
                <span class="property-readonly">"Trigger nodes start execution"</span>
            </div>
        }.into_any(),
        NodeVariant::Template { template } => view! {
            <div class="property-group">
                <label class="property-label">"Template"</label>
                <textarea
                    class="property-textarea"
                    rows="4"
                    prop:value={template.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::Template { template: new_value }));
                    }
                >{template.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::Retrieval { virtual_uri, limit } => {
            let virtual_uri_clone = virtual_uri.clone();
            let limit_clone = limit;
            view! {
                <div class="property-group">
                    <label class="property-label">"Virtual URI"</label>
                    <input
                        type="text"
                        class="property-input"
                        prop:value={virtual_uri_clone.clone()}
                        on:change={
                            let limit_for_uri = limit_clone;
                            move |ev| {
                                let new_value = event_target_value(&ev);
                                on_update_node.run((node_id, NodeVariant::Retrieval { virtual_uri: new_value, limit: limit_for_uri }));
                            }
                        }
                    />
                </div>
                <div class="property-group">
                    <label class="property-label">"Limit"</label>
                    <input
                        type="number"
                        class="property-input"
                        prop:value={limit_clone as f64}
                        min="1"
                        max="100"
                        on:change={
                            let virtual_uri_for_limit = virtual_uri_clone.clone();
                            move |ev| {
                                if let Ok(new_value) = event_target_value(&ev).parse::<usize>() {
                                    on_update_node.run((node_id, NodeVariant::Retrieval { virtual_uri: virtual_uri_for_limit.clone(), limit: new_value }));
                                }
                            }
                        }
                    />
                </div>
            }.into_any()
        },
        NodeVariant::Summarizer { max_length } => view! {
            <div class="property-group">
                <label class="property-label">"Max Length"</label>
                <input
                    type="number"
                    class="property-input"
                    prop:value={max_length as f64}
                    min="50"
                    max="2000"
                    on:change=move |ev| {
                        if let Ok(new_value) = event_target_value(&ev).parse::<u32>() {
                            on_update_node.run((node_id, NodeVariant::Summarizer { max_length: new_value }));
                        }
                    }
                />
            </div>
        }.into_any(),
        NodeVariant::PlannerAgent { goal } => view! {
            <div class="property-group">
                <label class="property-label">"Goal"</label>
                <textarea
                    class="property-textarea"
                    rows="2"
                    prop:value={goal.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::PlannerAgent { goal: new_value }));
                    }
                >{goal.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::ExecutorAgent { task } => view! {
            <div class="property-group">
                <label class="property-label">"Task"</label>
                <textarea
                    class="property-textarea"
                    rows="2"
                    prop:value={task.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::ExecutorAgent { task: new_value }));
                    }
                >{task.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::WebSearch { query, num_results } => view! {
            <div class="property-groups">
                <div class="property-group">
                    <label class="property-label">"Query"</label>
                    <input
                        type="text"
                        class="property-input"
                        prop:value={query.clone()}
                        on:change=move |ev| {
                            let new_value = event_target_value(&ev);
                            on_update_node.run((node_id, NodeVariant::WebSearch { query: new_value, num_results }));
                        }
                    />
                </div>
                <div class="property-group">
                    <label class="property-label">"Number of Results"</label>
                    <input
                        type="number"
                        class="property-input"
                        prop:value={num_results as f64}
                        min="1"
                        max="20"
                        on:change=move |ev| {
                            if let Ok(new_value) = event_target_value(&ev).parse::<u32>() {
                                on_update_node.run((node_id, NodeVariant::WebSearch { query: query.clone(), num_results: new_value }));
                            }
                        }
                    />
                </div>
            </div>
        }.into_any(),
        NodeVariant::CodeExecute { code, language } => {
            let code_for_lang = code.clone();
            let lang_for_code = language.clone();
            view! {
                <div class="property-groups">
                    <div class="property-group">
                        <label class="property-label">"Language"</label>
                        <input
                            type="text"
                            class="property-input"
                            prop:value={language.clone()}
                            on:change=move |ev| {
                                let new_value = event_target_value(&ev);
                                on_update_node.run((node_id, NodeVariant::CodeExecute { code: code_for_lang.clone(), language: new_value }));
                            }
                        />
                    </div>
                    <div class="property-group">
                        <label class="property-label">"Code"</label>
                        <textarea
                            class="property-textarea code"
                            rows="4"
                            prop:value={code.clone()}
                            on:change=move |ev| {
                                let new_value = event_target_value(&ev);
                                on_update_node.run((node_id, NodeVariant::CodeExecute { code: new_value, language: lang_for_code.clone() }));
                            }
                        >{code.clone()}</textarea>
                    </div>
                </div>
            }.into_any()
        }
        NodeVariant::IfCondition { branches } => view! {
            <div class="property-group">
                <label class="property-label">"Branches"</label>
                <input
                    type="number"
                    class="property-input"
                    prop:value={branches as f64}
                    min="2"
                    max="10"
                    on:change=move |ev| {
                        if let Ok(new_value) = event_target_value(&ev).parse::<u32>() {
                            on_update_node.run((node_id, NodeVariant::IfCondition { branches: new_value }));
                        }
                    }
                />
            </div>
        }.into_any(),
        NodeVariant::Loop { iterations } => view! {
            <div class="property-group">
                <label class="property-label">"Iterations"</label>
                <input
                    type="number"
                    class="property-input"
                    prop:value={iterations as f64}
                    min="1"
                    max="100"
                    on:change=move |ev| {
                        if let Ok(new_value) = event_target_value(&ev).parse::<u32>() {
                            on_update_node.run((node_id, NodeVariant::Loop { iterations: new_value }));
                        }
                    }
                />
            </div>
        }.into_any(),
        NodeVariant::ChatOutput { response } => view! {
            <div class="property-group">
                <label class="property-label">"Response"</label>
                <textarea
                    class="property-textarea"
                    rows="3"
                    prop:value={response.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::ChatOutput { response: new_value }));
                    }
                >{response.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::JsonOutput { schema } => view! {
            <div class="property-group">
                <label class="property-label">"JSON Schema"</label>
                <textarea
                    class="property-textarea code"
                    rows="4"
                    prop:value={schema.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::JsonOutput { schema: new_value }));
                    }
                >{schema.clone()}</textarea>
            </div>
        }.into_any(),
        NodeVariant::Index { collection } => view! {
            <div class="property-group">
                <label class="property-label">"Collection"</label>
                <input
                    type="text"
                    class="property-input"
                    prop:value={collection.clone()}
                    on:change=move |ev| {
                        let new_value = event_target_value(&ev);
                        on_update_node.run((node_id, NodeVariant::Index { collection: new_value }));
                    }
                />
            </div>
        }.into_any(),
        NodeVariant::ModelConfig { format, model_name, api_key, custom_url } => {
            view! {
                <div class="property-groups">
                    <div class="property-group">
                        <label class="property-label">"Format"</label>
                        <input
                            type="text"
                            class="property-input"
                            prop:value={format.clone()}
                            on:change={
                                let model_name = model_name.clone();
                                let api_key = api_key.clone();
                                let custom_url = custom_url.clone();
                                move |ev| {
                                    let new_value = event_target_value(&ev);
                                    on_update_node.run((node_id, NodeVariant::ModelConfig {
                                        format: new_value,
                                        model_name: model_name.clone(),
                                        api_key: api_key.clone(),
                                        custom_url: custom_url.clone(),
                                    }));
                                }
                            }
                        />
                    </div>
                    <div class="property-group">
                        <label class="property-label">"Model Name"</label>
                        <input
                            type="text"
                            class="property-input"
                            prop:value={model_name.clone()}
                            on:change={
                                let format = format.clone();
                                let api_key = api_key.clone();
                                let custom_url = custom_url.clone();
                                move |ev| {
                                    let new_value = event_target_value(&ev);
                                    on_update_node.run((node_id, NodeVariant::ModelConfig {
                                        format: format.clone(),
                                        model_name: new_value,
                                        api_key: api_key.clone(),
                                        custom_url: custom_url.clone(),
                                    }));
                                }
                            }
                        />
                    </div>
                    <div class="property-group">
                        <label class="property-label">"API Key"</label>
                        <input
                            type="password"
                            class="property-input"
                            prop:value={api_key.clone()}
                            on:change={
                                let format = format.clone();
                                let model_name = model_name.clone();
                                let custom_url = custom_url.clone();
                                move |ev| {
                                    let new_value = event_target_value(&ev);
                                    on_update_node.run((node_id, NodeVariant::ModelConfig {
                                        format: format.clone(),
                                        model_name: model_name.clone(),
                                        api_key: new_value,
                                        custom_url: custom_url.clone(),
                                    }));
                                }
                            }
                        />
                    </div>
                    <div class="property-group">
                        <label class="property-label">"Custom URL"</label>
                        <input
                            type="text"
                            class="property-input"
                            prop:value={custom_url.clone()}
                            on:change={
                                let format = format.clone();
                                let model_name = model_name.clone();
                                let api_key = api_key.clone();
                                move |ev| {
                                    let new_value = event_target_value(&ev);
                                    on_update_node.run((node_id, NodeVariant::ModelConfig {
                                        format: format.clone(),
                                        model_name: model_name.clone(),
                                        api_key: api_key.clone(),
                                        custom_url: new_value,
                                    }));
                                }
                            }
                        />
                    </div>
                </div>
            }.into_any()
        }
        NodeVariant::Model => view! {
            <div class="property-group">
                <span class="property-readonly">"Model node — config comes via port connection"</span>
            </div>
        }.into_any(),
    }
}
