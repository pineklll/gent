use crate::components::canvas::state::{BundledGroup, SavedSelection};
use crate::components::graph_section::GraphSection;
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tab {
    Palette,
    Plugins,
    Graph,
}

impl Default for Tab {
    fn default() -> Self {
        Self::Palette
    }
}

/// Node palette for the left panel
#[derive(Clone, Debug)]
pub struct NodeType {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
}

pub const NODE_TYPES: &[NodeType] = &[
    // Input
    NodeType {
        id: "text_input",
        name: "Text Input",
        category: "Input",
        description: "Text input from user",
    },
    NodeType {
        id: "file_input",
        name: "File Input",
        category: "Input",
        description: "Load file contents",
    },
    NodeType {
        id: "image_input",
        name: "Image Input",
        category: "Input",
        description: "Provides an image file path as output",
    },
    NodeType {
        id: "audio_input",
        name: "Audio Input",
        category: "Input",
        description: "Provides an audio file path as output",
    },
    NodeType {
        id: "trigger",
        name: "Trigger",
        category: "Input",
        description: "Click to start execution",
    },
    // Context
    NodeType {
        id: "template",
        name: "Template",
        category: "Context",
        description: "Jinja-style template",
    },
    NodeType {
        id: "retrieval",
        name: "Retrieval",
        category: "Context",
        description: "Vector DB retrieval",
    },
    NodeType {
        id: "index",
        name: "Index",
        category: "Context",
        description: "Index text into Chroma vector store",
    },
    NodeType {
        id: "summarizer",
        name: "Summarizer",
        category: "Context",
        description: "Summarize text",
    },
    // Agent
    NodeType {
        id: "planner_agent",
        name: "Planner Agent",
        category: "Agent",
        description: "Plans steps",
    },
    NodeType {
        id: "executor_agent",
        name: "Executor Agent",
        category: "Agent",
        description: "Executes tasks",
    },
    NodeType {
        id: "model",
        name: "Model",
        category: "Agent",
        description: "Call an LLM API with config from Model Config node",
    },
    NodeType {
        id: "model_config",
        name: "Model Config",
        category: "Agent",
        description: "Holds API configuration for Model node",
    },
    // Tool
    NodeType {
        id: "web_search",
        name: "Web Search",
        category: "Tool",
        description: "Search the web",
    },
    NodeType {
        id: "code_execute",
        name: "Code Execute",
        category: "Tool",
        description: "Run code",
    },
    // Control
    NodeType {
        id: "if_condition",
        name: "If / Condition",
        category: "Control",
        description: "Branch on condition",
    },
    NodeType {
        id: "loop",
        name: "Loop",
        category: "Control",
        description: "Iterate multiple times",
    },
    // Output
    NodeType {
        id: "text_output",
        name: "Text Output",
        category: "Output",
        description: "Display chat response",
    },
    NodeType {
        id: "json_output",
        name: "JSON Output",
        category: "Output",
        description: "Structured JSON output",
    },
];

fn get_nodes_by_category(category: &str) -> Vec<&'static NodeType> {
    NODE_TYPES
        .iter()
        .filter(|n| n.category == category)
        .collect()
}

#[component]
fn TabBar(active_tab: ReadSignal<Tab>, set_active_tab: WriteSignal<Tab>) -> impl IntoView {
    view! {
        <div class="tab-bar">
            <button
                class=move || format!("tab{}", if active_tab.get() == Tab::Palette { " tab-active" } else { "" })
                on:click={move |_| set_active_tab.set(Tab::Palette)}
            >
                "Palette"
            </button>
            <button
                class=move || format!("tab{}", if active_tab.get() == Tab::Plugins { " tab-active" } else { "" })
                on:click={move |_| set_active_tab.set(Tab::Plugins)}
            >
                "Plugins"
            </button>
            <button
                class=move || format!("tab{}", if active_tab.get() == Tab::Graph { " tab-active" } else { "" })
                on:click={move |_| set_active_tab.set(Tab::Graph)}
            >
                "Graph"
            </button>
        </div>
    }
}

#[component]
pub fn LeftPanel(
    /// Callback when drag starts from palette
    #[prop(default = None)]
    on_drag_start: Option<Callback<String>>,
    /// Saved selections signal (ReadSignal<Vec<SavedSelection>>)
    saved_selections: Signal<Vec<SavedSelection>>,
    /// Callback when a saved selection is loaded
    on_load_selection: Callback<SavedSelection>,
    /// Callback when a saved selection is deleted
    on_delete_selection: Callback<String>,
    /// Callback when a bundled group is loaded
    on_load_bundle: Callback<BundledGroup>,
    #[prop(default = None)]
    on_bundle_drag_start: Option<Callback<String>>,
    #[prop(default = None)]
    on_selection_drag_start: Option<Callback<String>>,
) -> impl IntoView {
    let (active_tab, set_active_tab) = signal(Tab::default());

    view! {
        <>
            <TabBar active_tab={active_tab.into()} set_active_tab={set_active_tab} />
            {move || match active_tab.get() {
                Tab::Palette => view! {
                    <NodePalette on_drag_start={on_drag_start} />
                }.into_any(),
                Tab::Plugins => view! { <crate::components::plugin_manager::PluginManager /> }.into_any(),
                Tab::Graph => view! {
                    <div class="panel-content">
                        <GraphSection
                            saved_selections={saved_selections}
                            on_load_selection={on_load_selection}
                            on_delete_selection={on_delete_selection}
                            on_load_bundle={on_load_bundle}
                            on_bundle_drag_start={on_bundle_drag_start}
                            on_selection_drag_start={on_selection_drag_start}
                        />
                    </div>
                }.into_any(),
            }}
        </>
    }
}

#[component]
pub fn PaletteSection(
    category: &'static str,
    nodes: Vec<&'static NodeType>,
    /// Callback when drag starts from palette
    #[prop(default = None)]
    on_drag_start: Option<Callback<String>>,
) -> impl IntoView {
    let items: Vec<_> = nodes
        .iter()
        .map(|node| {
            let node_id = node.id;
            view! {
                <div
                    class="palette-item"
                    data-node-type={node.id}
                    title={node.description}
                    on:mousedown={move |_ev| {
                        // Store in window for canvas to pick up
                        if let Some(window) = web_sys::window() {
                            let _ = js_sys::Reflect::set(
                                &window,
                                &"draggedNodeType".into(),
                                &node_id.into()
                            );
                        }
                        // Also call the callback if provided
                        if let Some(callback) = &on_drag_start {
                            callback.run(node_id.to_string());
                        }
                    }}
                >
                    {node.name}
                </div>
            }
        })
        .collect();

    view! {
        <div class="palette-section">
            <div class="palette-section-title">{category}</div>
            <div class="palette-items">
                {items}
            </div>
        </div>
    }
}

#[component]
pub fn NodePalette(
    #[prop(default = None)] on_drag_start: Option<Callback<String>>,
) -> impl IntoView {
    let categories = ["Input", "Context", "Agent", "Tool", "Control", "Output"];

    view! {
        <div class="panel-content">
            {categories.iter().filter_map(|category| {
                let nodes = get_nodes_by_category(category);
                if nodes.is_empty() {
                    None
                } else {
                    Some(view! {
                        <PaletteSection category={*category} nodes={nodes} on_drag_start={on_drag_start} />
                    })
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
