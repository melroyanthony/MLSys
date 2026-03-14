/// Solution -> JSON serialization.

use serde::Serialize;
use serde_json::Value;

use crate::models::Solution;

#[derive(Serialize)]
struct SolutionJson {
    subgraphs: Vec<Vec<usize>>,
    granularities: Vec<[i64; 3]>,
    tensors_to_retain: Vec<Vec<usize>>,
    traversal_orders: Vec<Value>,
    subgraph_latencies: Vec<f64>,
}

pub fn serialize_solution(solution: &Solution) -> Result<String, String> {
    let mut subgraphs = Vec::new();
    let mut granularities = Vec::new();
    let mut tensors_to_retain = Vec::new();
    let mut traversal_orders = Vec::new();
    let mut subgraph_latencies = Vec::new();

    for sg in &solution.subgraphs {
        subgraphs.push(sg.ops.clone());
        granularities.push([sg.granularity.w, sg.granularity.h, sg.granularity.k]);
        tensors_to_retain.push(sg.tensors_to_retain.clone());
        let to_val = match &sg.traversal_order {
            None => Value::Null,
            Some(order) => serde_json::to_value(order).unwrap(),
        };
        traversal_orders.push(to_val);
        subgraph_latencies.push(sg.subgraph_latency);
    }

    let json_struct = SolutionJson {
        subgraphs,
        granularities,
        tensors_to_retain,
        traversal_orders,
        subgraph_latencies,
    };

    serde_json::to_string_pretty(&json_struct).map_err(|e| format!("Serialize error: {e}"))
}
