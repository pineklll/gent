use std::collections::{HashMap, VecDeque};

/// Output mode for retrieval nodes
#[derive(Clone, Debug)]
pub enum RetrievalOutput {
    TextOnly,
    MetadataOnly,
    TextAndMetadata,
}

/// A single retrieval result
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RetrievalResult {
    pub text: String,
    pub metadata: serde_json::Value,
    pub score: f32,
}

/// WASM-compatible timestamp using js_sys::Date
#[derive(Clone, Copy, Debug)]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        Self(js_sys::Date::now() as u64)
    }

    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::from_millis(js_sys::Date::now() as u64 - self.0)
    }

    pub fn duration_since(&self, earlier: Timestamp) -> std::time::Duration {
        std::time::Duration::from_millis(self.0 - earlier.0)
    }
}

/// Trace level for styling
#[derive(Clone, Debug)]
pub enum TraceLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// A single entry in the execution trace
#[derive(Clone, Debug)]
pub struct TraceEntry {
    pub timestamp: Timestamp,
    pub message: String,
    pub level: TraceLevel,
}

impl TraceEntry {
    pub fn new(message: &str, level: TraceLevel) -> Self {
        Self {
            timestamp: Timestamp::now(),
            message: message.to_string(),
            level,
        }
    }
}

/// Execution status of a task
#[derive(Clone, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Waiting,
    Complete,
    Error,
}

/// A single task in the execution queue
#[derive(Clone, Debug)]
pub struct Task {
    pub id: String,
    pub node_id: u32,
    pub node_type: String,
    pub status: TaskStatus,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub parent_id: Option<String>,
    pub messages: Vec<TraceEntry>,
    pub result: Option<String>,
    pub waiting_on: Option<u32>, // node_id we're waiting for
}

impl Task {
    pub fn new(node_id: u32, node_type: &str, parent_id: Option<String>) -> Self {
        Self {
            id: format!("{}-{}", node_type, node_id),
            node_id,
            node_type: node_type.to_string(),
            status: TaskStatus::Pending,
            started_at: None,
            finished_at: None,
            parent_id,
            messages: Vec::new(),
            result: None,
            waiting_on: None,
        }
    }

    pub fn add_message(&mut self, msg: &str, level: TraceLevel) {
        self.messages.push(TraceEntry::new(msg, level));
    }
}

/// Execution engine state
#[derive(Clone, Debug)]
pub struct ExecutionState {
    pub tasks: Vec<Task>,
    pub running: bool,
}

impl ExecutionState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            running: false,
        }
    }
}

/// Find upstream node IDs connected to a node's input
pub fn get_upstream_nodes(
    connections: &[super::canvas::state::ConnectionState],
    node_id: u32,
) -> Vec<u32> {
    connections
        .iter()
        .filter(|c| c.target_node_id == node_id)
        .map(|c| c.source_node_id)
        .collect()
}

/// Execute nodes in topological order (BFS from trigger, including upstream dependencies)
pub fn execute_downstream_order(
    nodes: &[super::canvas::state::NodeState],
    connections: &[super::canvas::state::ConnectionState],
    trigger_id: u32,
) -> Vec<u32> {
    let mut in_degree: HashMap<u32, usize> = HashMap::new();
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();

    for node in nodes {
        in_degree.insert(node.id, 0);
        adj.insert(node.id, vec![]);
    }

    for conn in connections {
        if let Some(list) = adj.get_mut(&conn.source_node_id) {
            list.push(conn.target_node_id);
        }
        *in_degree.entry(conn.target_node_id).or_insert(0) += 1;
    }

    // 1. Find all nodes reachable downstream from the trigger
    let mut reachable: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut reach_queue: VecDeque<u32> = VecDeque::new();
    reach_queue.push_back(trigger_id);
    reachable.insert(trigger_id);

    while let Some(node_id) = reach_queue.pop_front() {
        if let Some(downstream_ids) = adj.get(&node_id) {
            for &downstream_id in downstream_ids {
                if reachable.insert(downstream_id) {
                    reach_queue.push_back(downstream_id);
                }
            }
        }
    }

    // 2. Expand reachable set to include all upstream dependencies
    loop {
        let mut added = false;
        for conn in connections {
            if reachable.contains(&conn.target_node_id) && reachable.insert(conn.source_node_id) {
                added = true;
            }
        }
        if !added {
            break;
        }
    }

    // 3. Topological sort restricted to the reachable set
    let mut queue: VecDeque<u32> = VecDeque::new();
    for node in nodes {
        if reachable.contains(&node.id) && in_degree.get(&node.id).copied().unwrap_or(0) == 0 {
            queue.push_back(node.id);
        }
    }

    let mut execution_order: Vec<u32> = vec![];

    while let Some(node_id) = queue.pop_front() {
        execution_order.push(node_id);

        if let Some(downstream_ids) = adj.get(&node_id) {
            for &downstream_id in downstream_ids {
                if reachable.contains(&downstream_id) {
                    *in_degree.entry(downstream_id).or_insert(0) -= 1;
                    if in_degree[&downstream_id] == 0 {
                        queue.push_back(downstream_id);
                    }
                }
            }
        }
    }

    execution_order
}

/// Execute a single node based on its type (synchronous only)
pub fn execute_node_sync(
    node: &super::canvas::state::NodeState,
    upstream_results: &HashMap<u32, String>,
    parent_id: Option<String>,
) -> (Task, Option<String>) {
    use super::canvas::state::NodeVariant;

    let mut task = Task::new(node.id, &node.node_type, parent_id);
    task.status = TaskStatus::Running;
    task.started_at = Some(Timestamp::now());

    let result = match node.node_type.as_str() {
        "trigger" => {
            task.add_message("Trigger fired", TraceLevel::Info);
            None
        }
        "text_input" => {
            if let NodeVariant::UserInput { text } = &node.variant {
                task.add_message(&format!("Text Input: {}", text), TraceLevel::Info);
                Some(text.clone())
            } else {
                task.add_message("Text Input (no text)", TraceLevel::Warn);
                Some(String::new())
            }
        }
        "text_output" => {
            let input = upstream_results.values().next().cloned().unwrap_or_default();
            task.add_message(&format!("Text Output received: {}", input), TraceLevel::Info);
            Some(input)
        }
        "json_output" => {
            let input = upstream_results
                .values()
                .next()
                .cloned()
                .unwrap_or_default();
            task.add_message(&format!("Output: {}", input), TraceLevel::Info);
            Some(input)
        }
        "web_search" => {
            task.add_message(
                "Web Search → { query: 'mock results', results: [] }",
                TraceLevel::Info,
            );
            Some(r#"{"query":"mock results","results":[]}"#.to_string())
        }
        "code_execute" => {
            task.add_message("Code Execute → (stubbed in MVP)", TraceLevel::Info);
            Some("code stubbed".to_string())
        }
        "image_input" => {
            if let NodeVariant::FileInput { path } = &node.variant {
                task.add_message(&format!("Image: {}", path), TraceLevel::Info);
                Some(path.clone())
            } else {
                task.add_message("Image Input (no path)", TraceLevel::Warn);
                Some(String::new())
            }
        }
        "audio_input" => {
            if let NodeVariant::FileInput { path } = &node.variant {
                task.add_message(&format!("Audio: {}", path), TraceLevel::Info);
                Some(path.clone())
            } else {
                task.add_message("Audio Input (no path)", TraceLevel::Warn);
                Some(String::new())
            }
        }
        "template" => {
            task.add_message("Template node", TraceLevel::Info);
            Some("template output".to_string())
        }
        "model_config" => {
            let config_json = if let NodeVariant::ModelConfig {
                format,
                model_name,
                api_key,
                custom_url,
            } = &node.variant
            {
                format!(
                    r#"{{"format":"{}","model_name":"{}","api_key":"{}","custom_url":"{}"}}"#,
                    format, model_name, api_key, custom_url
                )
            } else {
                r#"{"format":"openai","model_name":"","api_key":"","custom_url":""}"#.to_string()
            };
            task.add_message("Model Config node", TraceLevel::Info);
            Some(config_json)
        }
        "planner_agent" | "executor_agent" => {
            task.add_message("Agent processing...", TraceLevel::Info);
            upstream_results.values().next().cloned()
        }
        "if_condition" | "loop" => {
            task.add_message("Control flow stub - taking first branch", TraceLevel::Warn);
            upstream_results.values().next().cloned()
        }
        "index" => {
            let collection = if let NodeVariant::Index { collection } = &node.variant {
                collection.clone()
            } else {
                String::new()
            };
            let input = upstream_results.values().next().cloned().unwrap_or_default();
            task.add_message(
                &format!("Indexing to '{}': {} chars", collection, input.len()),
                TraceLevel::Info,
            );
            Some(input)
        }
        _ => {
            task.add_message(
                &format!("Unknown node type: {}", node.node_type),
                TraceLevel::Warn,
            );
            upstream_results.values().next().cloned()
        }
    };

    task.status = TaskStatus::Complete;
    task.finished_at = Some(Timestamp::now());
    if let Some(ref r) = result {
        task.result = Some(r.clone());
    }

    (task, result)
}
