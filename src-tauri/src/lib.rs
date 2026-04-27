use crate::plugins::commands::{
    call_plugin, list_plugins, load_plugin, load_plugin_from_path, unload_plugin, PluginState,
};
use crate::plugins::{PluginLoader, PluginRegistry};
use crate::scripts::commands::{list_scripts, read_script, run_script, save_script};
use std::process::Command;
use std::sync::Arc;

mod config;
mod llm;
mod plugins;
mod query_translation;
mod rag;
pub mod scripts;

#[tauri::command]
async fn llm_complete(
    config: llm::LlmConfig,
    input: llm::LlmInput,
) -> Result<llm::LlmOutput, String> {
    Ok(llm::llm_complete(config, input).await)
}

#[tauri::command]
fn get_llm_defaults() -> Result<config::AppConfig, String> {
    Ok(config::get_config().clone())
}

#[tauri::command]
fn show_main_window(window: tauri::Window) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

#[tauri::command]
fn execute_code(code: String) -> Result<String, String> {
    // Run via sh on mac/linux, cmd on windows
    #[cfg(target_os = "windows")]
    let output = Command::new("cmd").args(["/C", &code]).output();

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh").args(["-c", &code]).output();

    match output {
        Ok(out) => {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn import_graph(path: String) -> Result<String, String> {
    use std::fs;
    fs::read_to_string(&path).map_err(|e| format!("failed to read file: {}", e))
}

#[tauri::command]
async fn export_graph(path: String, json: String) -> Result<(), String> {
    use std::fs;
    fs::write(&path, json).map_err(|e| format!("failed to write file: {}", e))
}

#[tauri::command(rename_all = "snake_case")]
async fn retrieve(
    virtual_uri: String,
    query: String,
    top_k: usize,
) -> Result<Vec<rag::RetrievalResult>, String> {
    rag::retrieve(virtual_uri, query, top_k).await
}

#[tauri::command(rename_all = "snake_case")]
async fn multi_query(
    query: String,
    num_queries: usize,
    format: String,
    model_name: String,
    api_key: String,
    custom_url: String,
) -> Result<query_translation::MultiQueryResponse, String> {
    let config = llm::LlmConfig {
        format,
        model_name,
        api_key,
        custom_url,
    };
    query_translation::multi_query(config, query, num_queries).await
}

#[tauri::command(rename_all = "snake_case")]
async fn step_back(
    query: String,
    format: String,
    model_name: String,
    api_key: String,
    custom_url: String,
) -> Result<query_translation::StepBackResponse, String> {
    let config = llm::LlmConfig {
        format,
        model_name,
        api_key,
        custom_url,
    };
    query_translation::step_back(config, query).await
}

#[tauri::command(rename_all = "snake_case")]
async fn rag_fusion(
    query: String,
    virtual_uri: String,
    num_queries: usize,
    limit_per_query: usize,
    k: usize,
    format: String,
    model_name: String,
    api_key: String,
    custom_url: String,
) -> Result<query_translation::RagFusionResponse, String> {
    let config = llm::LlmConfig {
        format,
        model_name,
        api_key,
        custom_url,
    };
    query_translation::rag_fusion(config, query, virtual_uri, num_queries, limit_per_query, k).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load XDG config (~/.config/gent/config.toml)
    config::load_config();

    // Initialize Rune engine singleton
    let rune_engine =
        crate::scripts::engine::RuneEngine::new().expect("failed to initialize Rune engine");
    crate::scripts::engine::RUNE_ENGINE
        .set(Arc::new(rune_engine))
        .expect("Rune engine already initialized");

    let plugin_state = Arc::new(PluginState {
        registry: PluginRegistry::new(),
        loader: PluginLoader::new(),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(plugin_state)
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            execute_code,
            llm_complete,
            get_llm_defaults,
            load_plugin,
            load_plugin_from_path,
            list_plugins,
            unload_plugin,
            call_plugin,
            // Script commands
            list_scripts,
            read_script,
            save_script,
            run_script,
            // Import/Export commands
            import_graph,
            export_graph,
            // RAG commands
            retrieve,
            multi_query,
            step_back,
            rag_fusion,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
