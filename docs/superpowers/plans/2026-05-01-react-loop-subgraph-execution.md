# ReAct Loop Subgraph Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor ReAct loop execution so the ReAct node is excluded from topological sort (it forms a cycle with action_result), and instead executes its connected subgraph as a unit on each iteration.

**Architecture:** Kahn's algorithm fails on cycles. We detect strongly connected components (SCCs) via Tarjan's algorithm. Any SCC containing a ReAct loop node is treated as a **loop group** — those nodes are excluded from the main execution order, and the orchestrating ReAct node manually executes its subgraph on each iteration.

**Tech Stack:** Leptos 0.8 WASM frontend, Tauri 2.x, Rust for SCC detection

---

## File Structure

| File | Changes |
|------|---------|
| `src/components/execution_engine.rs` | Add Tarjan SCC detection, refactor `execute_downstream_order` to accept SCC info, add `build_execution_plan` that separates loop groups |
| `src/components/app_layout.rs` | Remove ReAct node from main topo sort, wire ReAct to use subgraph execution internally |

---

## Task 1: Tarjan SCC Detection

**Files:**
- Modify: `src/components/execution_engine.rs`

- [ ] **Step 1: Add Tarjan SCC types**

In `src/components/execution_engine.rs`, after the imports, add:

```rust
/// A node with its DFS discovery/discovery metadata for Tarjan's algorithm
#[derive(Clone, Debug)]
struct SccNode {
    node_id: u32,
    index: Option<usize>,    // discovery time (None = not in stack)
    lowlink: usize,
    on_stack: bool,
}

/// Find strongly connected components using Tarjan's algorithm
/// Returns a vector of SCCs, each SCC is a set of node IDs
fn find_sccs(
    nodes: &[super::canvas::state::NodeState],
    connections: &[super::canvas::state::ConnectionState],
) -> Vec<std::collections::HashSet<u32>> {
    let mut index: usize = 0;
    let mut stack: Vec<u32> = Vec::new();
    let mut sccs: Vec<std::collections::HashSet<u32>> = Vec::new();

    // Build adjacency list
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for node in nodes {
        adj.insert(node.id, vec![]);
    }
    for conn in connections {
        if let Some(list) = adj.get_mut(&conn.source_node_id) {
            list.push(conn.target_node_id);
        }
    }

    let mut node_data: HashMap<u32, SccNode> = nodes
        .iter()
        .map(|n| {
            (
                n.id,
                SccNode {
                    node_id: n.id,
                    index: None,
                    lowlink: 0,
                    on_stack: false,
                },
            )
        })
        .collect();

    fn strongconnect(
        node_id: u32,
        adj: &HashMap<u32, Vec<u32>>,
        node_data: &mut HashMap<u32, SccNode>,
        stack: &mut Vec<u32>,
        sccs: &mut Vec<std::collections::HashSet<u32>>,
        index: &mut usize,
    ) {
        // Set the depth index for node_id
        node_data.get_mut(&node_id).unwrap().index = Some(*index);
        node_data.get_mut(&node_id).unwrap().lowlink = *index;
        *index += 1;
        stack.push(node_id);
        node_data.get_mut(&node_id).unwrap().on_stack = true;

        // Consider successors
        if let Some(successors) = adj.get(&node_id) {
            for &succ in successors {
                if let Some(succ_index) = node_data.get(&succ).and_then(|n| n.index) {
                    // Successor is on stack, so it is in current SCC
                    if succ_index.is_some() {
                        let succ_lowlink = node_data.get(&succ).unwrap().lowlink;
                        let node_lowlink = &mut node_data.get_mut(&node_id).unwrap().lowlink;
                        *node_lowlink = node_lowlink.min(succ_lowlink);
                    }
                } else {
                    // Successor not yet visited, recurse
                    strongconnect(succ, adj, node_data, stack, sccs, index);
                    let succ_lowlink = node_data.get(&succ).unwrap().lowlink;
                    let node_lowlink = &mut node_data.get_mut(&node_id).unwrap().lowlink;
                    *node_lowlink = node_lowlink.min(succ_lowlink);
                }
            }
        }

        // Check if node_id is a root node
        let node_lowlink = node_data.get(&node_id).unwrap().lowlink;
        let node_index = node_data.get(&node_id).unwrap().index.unwrap();
        if node_lowlink == node_index {
            // Start a new SCC
            let mut scc = std::collections::HashSet::new();
            loop {
                let w = stack.pop().unwrap();
                node_data.get_mut(&w).unwrap().on_stack = false;
                scc.insert(w);
                if w == node_id {
                    break;
                }
            }
            sccs.push(scc);
        }
    }

    for &node_id in nodes.iter().map(|n| n.id).collect::<Vec<_>>().iter() {
        if node_data.get(&node_id).unwrap().index.is_none() {
            strongconnect(node_id, &adj, &mut node_data, &mut stack, &mut sccs, &mut index);
        }
    }

    sccs
}
```

- [ ] **Step 2: Add `is_loop_node_type` helper**

After `find_sccs`, add:

```rust
/// Returns true if node type forms a cycle and should be excluded from topo sort
fn is_loop_node_type(node_type: &str) -> bool {
    matches!(node_type, "react_loop")
}
```

- [ ] **Step 3: Add `ExecutionPlan` struct**

After `is_loop_node_type`:

```rust
/// Result of building an execution plan: separates nodes into linear order vs loop groups
#[derive(Clone, Debug)]
pub struct ExecutionPlan {
    /// Nodes to execute in topological order (no cycles)
    pub linear_order: Vec<u32>,
    /// Node IDs that are part of cycle SCCs (to be executed as loop units)
    pub loop_group_ids: Vec<u32>,
}

/// Build execution plan: Kahn topo sort with SCC-aware loop group extraction
pub fn build_execution_plan(
    nodes: &[super::canvas::state::NodeState],
    connections: &[super::canvas::state::ConnectionState],
    trigger_id: u32,
) -> ExecutionPlan {
    // Step 1: Find all SCCs
    let sccs = find_sccs(nodes, connections);

    // Step 2: Identify SCCs that contain loop nodes (react_loop)
    let loop_sccs: Vec<std::collections::HashSet<u32>> = sccs
        .into_iter()
        .filter(|scc| scc.iter().any(|&id| {
            nodes.iter()
                .find(|n| n.id == id)
                .map(|n| is_loop_node_type(&n.node_type))
                .unwrap_or(false)
        }))
        .collect();

    let loop_group_ids: Vec<u32> = loop_sccs.iter().flatten().copied().collect();
    let loop_group_set: std::collections::HashSet<u32> = loop_group_ids.iter().copied().collect();

    // Step 3: Build linear topo sort excluding loop group nodes
    let linear_order = topological_sort_excluding(nodes, connections, trigger_id, &loop_group_set);

    ExecutionPlan {
        linear_order,
        loop_group_ids,
    }
}

/// Topological sort excluding nodes in `exclude` set
fn topological_sort_excluding(
    nodes: &[super::canvas::state::NodeState],
    connections: &[super::canvas::state::ConnectionState],
    trigger_id: u32,
    exclude: &std::collections::HashSet<u32>,
) -> Vec<u32> {
    let mut in_degree: HashMap<u32, usize> = HashMap::new();
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();

    for node in nodes {
        if !exclude.contains(&node.id) {
            in_degree.insert(node.id, 0);
            adj.insert(node.id, vec![]);
        }
    }

    for conn in connections {
        if !exclude.contains(&conn.source_node_id) && !exclude.contains(&conn.target_node_id) {
            if let Some(list) = adj.get_mut(&conn.source_node_id) {
                list.push(conn.target_node_id);
            }
            *in_degree.entry(conn.target_node_id).or_insert(0) += 1;
        }
    }

    // Find reachable from trigger (within non-excluded nodes)
    let mut reachable: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut queue: Vec<u32> = vec![trigger_id];

    while let Some(node_id) = queue.pop() {
        if exclude.contains(&node_id) || !reachable.insert(node_id) {
            continue;
        }
        if let Some(downstream) = adj.get(&node_id) {
            queue.extend(downstream.iter().copied());
        }
    }

    // Expand reachable to include upstream non-excluded deps
    loop {
        let mut added = false;
        for conn in connections {
            if !exclude.contains(&conn.source_node_id) &&
               !exclude.contains(&conn.target_node_id) &&
               reachable.contains(&conn.target_node_id) &&
               reachable.insert(conn.source_node_id) {
                added = true;
            }
        }
        if !added { break; }
    }

    let mut q: VecDeque<u32> = VecDeque::new();
    for node_id in &reachable {
        if in_degree.get(node_id).copied().unwrap_or(0) == 0 {
            q.push_back(*node_id);
        }
    }

    let mut order = vec![];
    while let Some(node_id) = q.pop_front() {
        order.push(node_id);
        if let Some(downstream) = adj.get(&node_id) {
            for &next in downstream {
                if reachable.contains(&next) {
                    *in_degree.entry(next).or_insert(0) -= 1;
                    if in_degree[&next] == 0 {
                        q.push_back(next);
                    }
                }
            }
        }
    }

    order
}
```

- [ ] **Step 4: Export `ExecutionPlan` from execution_engine**

Make sure `ExecutionPlan`, `build_execution_plan`, and `find_sccs` are listed in the module's `pub use` statements or are public.

- [ ] **Step 5: Commit**

```bash
git add src/components/execution_engine.rs
git commit -m "feat(execution): add Tarjan SCC detection and ExecutionPlan struct"
```

---

## Task 2: Wire app_layout to Use ExecutionPlan + Subgraph Execution for ReAct

**Files:**
- Modify: `src/components/app_layout.rs:905-960` (handle_trigger execution planning)
- Modify: `src/components/app_layout.rs:1251-1520` (ReAct loop execution block)

- [ ] **Step 1: Import ExecutionPlan in app_layout**

Find the `execution_engine` import in app_layout.rs and add `ExecutionPlan` and `build_execution_plan` to the import:

```rust
use crate::components::execution_engine::{
    build_execution_plan, execute_downstream_order, execute_node_sync, ExecutionPlan,
    ExecutionState,
};
```

- [ ] **Step 2: Replace topo sort call with build_execution_plan**

In `handle_trigger` around line 924, replace:

```rust
let exec_order_ids =
    execute_downstream_order(&nodes_snapshot, &connections_snapshot, node_id);
```

With:

```rust
let exec_plan = build_execution_plan(&nodes_snapshot, &connections_snapshot, node_id);
let exec_order_ids = exec_plan.linear_order.clone();
let loop_group_ids = exec_plan.loop_group_ids.clone();

web_sys::console::log_1(&format!("DEBUG: exec_order_ids = {:?}", exec_order_ids).into());
web_sys::console::log_1(&format!("DEBUG: loop_group_ids = {:?}", loop_group_ids).into());
```

- [ ] **Step 3: Add loop group processing in main execution loop**

In the `for exec_node_id in exec_order_ids` loop, after the `else if node.node_type == "react_loop"` block is removed from the main loop, we need to handle loop groups separately.

Actually — the cleaner approach is to **handle react_loop nodes in the execution loop normally** (they appear in `exec_order_ids` if they're not in a cycle), but **the action_result back-edge is what creates the cycle**. So we need to modify the execution loop to:

1. When processing a ReAct node, skip the `action_result` input when computing upstream (it's a back-edge that feeds into the next iteration)
2. After ReAct finishes all iterations, it fires `done` downstream normally

Let me reconsider. The current problem is:
- ReAct → action_trigger → retrieval → ... → action_result → ReAct creates a cycle
- `execute_downstream_order` from trigger includes ReAct but the cycle causes incorrect ordering

The fix in **Task 1** (excluding SCC nodes from topo sort) means ReAct won't appear in `linear_order`. So we need to **execute ReAct separately** after the linear order completes:

In `handle_trigger`, after the main `for exec_node_id in exec_order_ids` loop, add:

```rust
// After linear execution order completes, execute loop group nodes
for &loop_node_id in &loop_group_ids {
    let loop_node = nodes_snapshot.iter().find(|n| n.id == loop_node_id);
    if let Some(node) = loop_node {
        if node.node_type == "react_loop" {
            // Get input_text from upstream (linear nodes already executed)
            let input_text = connections_snapshot
                .iter()
                .find(|c| c.target_node_id == loop_node_id && c.target_port_name == "input")
                .and_then(|c| node_results.get(&c.source_node_id))
                .cloned()
                .unwrap_or_default();

            // Get config similarly (from ModelConfig which should be in linear order)
            let config_json = connections_snapshot
                .iter()
                .find(|c| c.target_node_id == loop_node_id && c.target_port_name == "config")
                .and_then(|c| node_results.get(&c.source_node_id))
                .cloned()
                .unwrap_or_else(|| r#"{"format":"openai","model_name":"","api_key":"","custom_url":""}"#.to_string());

            let get_json_str = |json: &str, key: &str| -> String {
                let pattern = format!(r#""{}":"#, key);
                json.find(&pattern)
                    .map(|i| {
                        let start = i + pattern.len();
                        let rest = &json[start..];
                        let end = rest.find(',').or_else(|| rest.find('}')).unwrap_or(rest.len());
                        rest[..end].trim().trim_matches('"').to_string()
                    })
                    .unwrap_or_default()
            };

            let config = crate::components::canvas::state::ModelConfig {
                format: get_json_str(&config_json, "format"),
                model_name: get_json_str(&config_json, "model_name"),
                api_key: get_json_str(&config_json, "api_key"),
                custom_url: get_json_str(&config_json, "custom_url"),
            };

            // Execute ReAct loop (same body as current, but uses loop_node_id, input_text, config)
            // ReAct internally fires action_trigger → executes subgraph → collects action_result → iterates
            // After max_iterations or FINAL: fires done trigger downstream (into nodes that ARE in linear_order)

            let mut loop_task = Task::new(loop_node_id, "react_loop", parent_id.clone());
            // ... ReAct iteration loop (same as current implementation) ...
            // Key difference: subgraph execution (action_trigger → downstream → action_result) happens per iteration

            node_results.insert(loop_node_id, final_answer);
            exec.tasks.push(loop_task);
        }
    }
}
```

- [ ] **Step 4: Refactor ReAct action execution to use subgraph**

In the ReAct iteration loop, the current code manually finds `action_trigger` connection and executes retrieval nodes. Replace this with a cleaner subgraph execution pattern:

```rust
// action_trigger port connects to the entry point of the subgraph
let action_trigger_conn = connections_snapshot
    .iter()
    .find(|c| c.source_node_id == exec_node_id && c.source_port_name == "action_trigger");

if let Some(trigger_conn) = action_trigger_conn {
    let subgraph_entry = trigger_conn.target_node_id;

    // Execute the subgraph starting from action_trigger target
    // This is the key: execute_downstream_order on the SUBGRAPH (not the whole graph)
    let subgraph_order = execute_downstream_order(
        &nodes_snapshot,
        &connections_snapshot,
        subgraph_entry,
    );

    for subgraph_node_id in subgraph_order {
        if subgraph_node_id == subgraph_entry {
            // Execute entry node with action_input as its input
            if let Some(node) = nodes_snapshot.iter().find(|n| n.id == subgraph_node_id) {
                let mut subgraph_task = Task::new(subgraph_node_id, &node.node_type, Some(loop_task.id.clone()));
                subgraph_task.status = TaskStatus::Running;
                // ... execute based on node type ...
            }
            continue;
        }

        // Execute remaining subgraph nodes normally (using upstream results)
        if let Some(node) = nodes_snapshot.iter().find(|n| n.id == subgraph_node_id) {
            let upstream_ids = get_upstream_nodes(&connections_snapshot, subgraph_node_id);
            let upstream: HashMap<u32, String> = upstream_ids
                .into_iter()
                .filter_map(|id| node_results.get(&id).map(|r| (id, r.clone())))
                .collect();

            let (task, result) = execute_node_sync(node, &upstream, Some(loop_task.id.clone()));
            if let Some(r) = result {
                node_results.insert(subgraph_node_id, r);
            }
            exec.tasks.push(task);
        }
    }

    // action_result should be connected from the LAST node of the subgraph
    // Find the node that connects to action_result
    let action_result_conn = connections_snapshot
        .iter()
        .find(|c| c.target_node_id == exec_node_id && c.target_port_name == "action_result");

    if let Some(result_conn) = action_result_conn {
        let result_source_id = result_conn.source_node_id;
        if let Some(observation) = node_results.get(&result_source_id) {
            // observation becomes the action_result for this iteration
            history.push_str(&format!(
                "Thought: {}\nAction: {}\nAction Input: {}\nObservation: {}\n",
                llm_output, action, action_input, observation
            ));
        }
    }
}
```

- [ ] **Step 5: Fire done trigger after loop completes**

After the ReAct loop finishes (max iterations or FINAL), fire the `done` trigger to downstream nodes (which are in `linear_order` and will execute in sequence):

```rust
// Fire done trigger to downstream nodes (these are in linear_order)
let done_conn = connections_snapshot
    .iter()
    .find(|c| c.source_node_id == exec_node_id && c.source_port_name == "done");

if let Some(done_conn) = done_conn {
    let done_target = done_conn.target_node_id;
    // done_target and its downstream are already in exec_order_ids, so they will
    // execute naturally in the main execution loop AFTER loop_group_ids processing
    // But we need to make sure the input to done_target is set correctly
    // Insert the final answer as the input to done_target
    if let Some(final_val) = &final_answer {
        node_results.insert(done_target, final_val.clone());
    }
}
```

- [ ] **Step 6: Commit**

```bash
git add src/components/app_layout.rs
git commit -m "feat(execution): wire ExecutionPlan + ReAct subgraph execution"
```

---

## Task 3: Test the ReAct Loop with SCC-Aware Execution

- [ ] **Step 1: Build**

Run: `cd /c/Users/DELL/Documents/github/gent && trunk serve`
Expected: No compilation errors

- [ ] **Step 2: Test scenario**

1. Open http://localhost:1420
2. Wire: TextInput → ReAct (input), ModelConfig → ReAct (config)
3. Wire: ReAct (action_trigger) → Retrieval
4. Wire: Retrieval → ReAct (action_result)
5. Wire: ReAct (done) → TextOutput
6. Click Trigger

Expected:
- `exec_order_ids` excludes ReAct (it's in a cycle)
- `loop_group_ids` = [ReAct node id]
- Linear nodes (TextInput, ModelConfig, Retrieval, TextOutput) execute in topo order
- ReAct executes last (after linear order), runs its iterations
- Each iteration fires action_trigger → retrieves → observation feeds back via action_result
- After FINAL, fires done → TextOutput

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "test(react-loop): SCC-aware execution with subgraph test"
```

---

## Self-Review Checklist

1. **Spec coverage:** All requirements from react-loop-implementation are preserved:
   - ReAct iterations with max_iterations limit ✅
   - LLM-driven FINAL vs Action parsing ✅
   - action_trigger → subgraph execution ✅
   - action_result feeds back to next iteration ✅
   - done trigger fires downstream ✅

2. **Placeholder scan:** No placeholders found

3. **Type consistency:**
   - `ExecutionPlan { linear_order, loop_group_ids }` — used in both execution_engine.rs (def) and app_layout.rs (use)
   - `build_execution_plan(nodes, connections, trigger_id) -> ExecutionPlan` — consistent signature
   - Tarjan's algorithm correctly computes SCCs
