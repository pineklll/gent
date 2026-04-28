# ReAct Loop Node Design

## Goal

Implement a ReAct (Reasoning + Acting) loop node that orchestrates a think→act→observe cycle, delegating actions to a configurable subgraph while using an LLM to decide when to stop.

## Architecture

### Node: ReAct Loop

**Ports:**
| Port | Direction | Type | Purpose |
|------|-----------|------|---------|
| `input` | In | Text | Initial question |
| `config` | In | Text | ModelConfig node connection for LLM |
| `action_trigger` | Out | Trigger | Fires to execute action subgraph |
| `action_result` | In | Text | Receives action result → becomes observation |
| `done` | Out | Trigger | Fires when LLM outputs FINAL or max iterations |
| `output` | Out | Text | Final answer |

**Node Variant:**
```rust
NodeVariant::ReActLoop {
    action_type: String,       // "retrieval" | "web_search" | "code_exec"
    max_iterations: u32,       // Safety limit, default 5
}
```

**Default Ports for "react_loop" type:**
```rust
"react_loop" => vec![
    Port { name: "input".into(), port_type: PortType::Text, direction: PortDirection::In },
    Port { name: "config".into(), port_type: PortType::Text, direction: PortDirection::In },
    Port { name: "action_trigger".into(), port_type: PortType::Trigger, direction: PortDirection::Out },
    Port { name: "action_result".into(), port_type: PortType::Text, direction: PortDirection::In },
    Port { name: "done".into(), port_type: PortType::Trigger, direction: PortDirection::Out },
    Port { name: "output".into(), port_type: PortType::Text, direction: PortDirection::Out },
]
```

## Per-Iteration Flow

```
1. Build prompt with history:
   Question: {input}
   {history}
   Thought: {LLM reasons}

2. Call LLM with config (read from config port via get_json_str)

3. Parse LLM output:
   - If starts with "FINAL:" → extract answer after "FINAL:", fire done, set output
   - If "Action: {tool}\nAction Input: {params}" → fire action_trigger with params, wait for action_result, append observation to history, repeat
   - If parse error → append raw output as thought, repeat

4. If max_iterations reached → fire done with current output
```

## History Format

```
Thought: I need to find information about X
Action: retrieval
Action Input: {"virtual_uri": "viking://...", "query": "X", "limit": 5}
Observation: Found 3 results about X...
Thought: I now have enough information
FINAL: The answer is X because...
```

## Action Type Support

| action_type | Status | Implementation |
|-------------|--------|----------------|
| `retrieval` | MVP | Uses existing `call_retrieve` function |
| `web_search` | Future | Stub - return error |
| `code_exec` | Future | Stub - return error |

## Prompt Template

```rust
const REACT_PROMPT_TEMPLATE: &str = r#"Answer the following questions as best you can. You have access to the following tools:

- retrieval: Query a vector database. Input: {"virtual_uri": "...", "query": "...", "limit": N}

Use the following format:

Question: the input question you must answer
{history}
Thought: you should always think about what to do
Action: the action to take (must be one of: retrieval, FINAL)
Action Input: the input to the action
Observation: the result of the action
... (this Thought/Action/Action Input/Observation can repeat N times)

When you know the final answer, output:
FINAL: your final answer here"#;
```

## Execution State

The loop maintains state across iterations in `node_results`:
- Key: node_id
- Value: JSON with `{ history: String, iteration: u32 }`

## Files to Modify

| File | Changes |
|------|---------|
| `src/components/canvas/state.rs` | Add `ReActLoop` variant, add `react_loop` ports |
| `src/components/nodes/node.rs` | Render ReActLoop body with action_type dropdown and max_iterations |
| `src/components/execution_engine.rs` | Add `react_loop` execution case |
| `src/components/app_layout.rs` | Wire ReAct loop execution, handle action_trigger/result |

## Error Handling

- Unknown action_type: return error message, don't loop
- LLM call fails: append error to history, allow retry
- Max iterations reached: fire done with current output, log warning
- action_trigger has no connected node: fire done immediately with error

## UI Rendering

```rust
NodeVariant::ReActLoop { action_type, max_iterations } => {
    let action_type_cb = on_text_change.clone(); // reuse for action_type
    let max_iter_cb = on_limit_change.clone();
    view! {
        <div class="node-variant-fields">
            <div class="node-variant-field">
                <label>"Action"</label>
                <select class="node-variant-select">
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
                />
            </div>
        </div>
    }.into_any()
}
```
