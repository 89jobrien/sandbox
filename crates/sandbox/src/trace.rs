use serde::Serialize;
use std::time::SystemTime;

use crate::capabilities::Cap;

#[derive(Debug, Clone, Serialize)]
pub struct TraceEvent {
    pub timestamp: SystemTime,
    pub command: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub duration_us: u64,
    pub capabilities_used: Vec<Cap>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineTrace {
    pub nodes: Vec<PipelineNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineNode {
    pub cmd: String,
    pub args: Vec<String>,
    pub duration_us: u64,
    pub stdin_bytes: usize,
    pub stdout_bytes: usize,
    pub exit_code: i32,
}
