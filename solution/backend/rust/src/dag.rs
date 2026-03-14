/// DAG utilities: topological sort (Kahn's), adjacency, graph input/output identification.

use std::collections::{HashSet, VecDeque};

use crate::models::Problem;

#[derive(Debug)]
pub struct DagInfo {
    pub num_ops: usize,
    pub num_tensors: usize,
    /// op -> list of successor ops (ops that consume this op's outputs)
    pub successors: Vec<Vec<usize>>,
    /// op -> list of predecessor ops (ops whose outputs feed this op)
    pub predecessors: Vec<Vec<usize>>,
    /// tensor_id -> op that produces it (-1 equivalent = None for graph inputs)
    pub tensor_producer: Vec<Option<usize>>,
    /// tensor_id -> ops that consume it
    pub tensor_consumers: Vec<Vec<usize>>,
    /// tensor indices with no producer (graph inputs, start in slow memory)
    pub graph_inputs: HashSet<usize>,
    /// tensor indices with no consumer (graph outputs, must end in slow memory)
    pub graph_outputs: HashSet<usize>,
    /// topologically sorted op indices
    pub topo_order: Vec<usize>,
}

impl DagInfo {
    pub fn build(problem: &Problem) -> Result<Self, String> {
        let num_ops = problem.ops.len();
        let num_tensors = problem.tensors.len();

        let mut tensor_producer: Vec<Option<usize>> = vec![None; num_tensors];
        let mut tensor_consumers: Vec<Vec<usize>> = vec![vec![]; num_tensors];

        for (op_idx, op) in problem.ops.iter().enumerate() {
            for &out_t in &op.outputs {
                tensor_producer[out_t] = Some(op_idx);
            }
            for &in_t in &op.inputs {
                tensor_consumers[in_t].push(op_idx);
            }
        }

        // Graph inputs: tensors with no producer
        let graph_inputs: HashSet<usize> = (0..num_tensors)
            .filter(|&t| tensor_producer[t].is_none())
            .collect();

        // Graph outputs: tensors with no consumer
        let graph_outputs: HashSet<usize> = (0..num_tensors)
            .filter(|&t| tensor_consumers[t].is_empty())
            .collect();

        // Build op-level adjacency
        let mut successors: Vec<Vec<usize>> = vec![vec![]; num_ops];
        let mut predecessors: Vec<Vec<usize>> = vec![vec![]; num_ops];
        let mut in_degree: Vec<usize> = vec![0; num_ops];

        for (op_idx, op) in problem.ops.iter().enumerate() {
            for &out_t in &op.outputs {
                for &consumer_op in &tensor_consumers[out_t] {
                    successors[op_idx].push(consumer_op);
                    predecessors[consumer_op].push(op_idx);
                    in_degree[consumer_op] += 1;
                }
            }
        }

        // Kahn's algorithm for topological sort
        let mut queue: VecDeque<usize> = (0..num_ops)
            .filter(|&i| in_degree[i] == 0)
            .collect();

        let mut topo_order: Vec<usize> = Vec::with_capacity(num_ops);
        while let Some(op_idx) = queue.pop_front() {
            topo_order.push(op_idx);
            for &succ in &successors[op_idx] {
                in_degree[succ] -= 1;
                if in_degree[succ] == 0 {
                    queue.push_back(succ);
                }
            }
        }

        if topo_order.len() != num_ops {
            return Err("DAG has a cycle".to_string());
        }

        Ok(DagInfo {
            num_ops,
            num_tensors,
            successors,
            predecessors,
            tensor_producer,
            tensor_consumers,
            graph_inputs,
            graph_outputs,
            topo_order,
        })
    }

    /// Given a set of op indices (subgraph), return the boundary input tensor indices:
    /// tensors consumed by ops in the subgraph that are NOT produced within the subgraph.
    pub fn boundary_inputs(
        &self,
        problem: &Problem,
        subgraph_ops: &[usize],
    ) -> Vec<usize> {
        let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
        let mut result: HashSet<usize> = HashSet::new();

        for &op_idx in subgraph_ops {
            for &t in &problem.ops[op_idx].inputs {
                // Boundary input if producer is not in the subgraph
                let produced_inside = self.tensor_producer[t]
                    .map(|prod| op_set.contains(&prod))
                    .unwrap_or(false);
                if !produced_inside {
                    result.insert(t);
                }
            }
        }

        let mut v: Vec<usize> = result.into_iter().collect();
        v.sort_unstable();
        v
    }

    /// Given a set of op indices, return boundary output tensor indices:
    /// tensors produced by ops in the subgraph that are either graph outputs
    /// OR consumed by ops OUTSIDE the subgraph.
    pub fn boundary_outputs(
        &self,
        problem: &Problem,
        subgraph_ops: &[usize],
    ) -> Vec<usize> {
        let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
        let mut result: HashSet<usize> = HashSet::new();

        for &op_idx in subgraph_ops {
            for &t in &problem.ops[op_idx].outputs {
                // Check if it's a graph output
                if self.graph_outputs.contains(&t) {
                    result.insert(t);
                    continue;
                }
                // Check if any consumer is outside the subgraph
                let has_external_consumer = self.tensor_consumers[t]
                    .iter()
                    .any(|c| !op_set.contains(c));
                if has_external_consumer {
                    result.insert(t);
                }
            }
        }

        let mut v: Vec<usize> = result.into_iter().collect();
        v.sort_unstable();
        v
    }

    /// Returns the set of tensors that are purely ephemeral within a subgraph:
    /// produced AND all consumers are within the same subgraph.
    pub fn ephemeral_tensors(
        &self,
        problem: &Problem,
        subgraph_ops: &[usize],
    ) -> HashSet<usize> {
        let op_set: HashSet<usize> = subgraph_ops.iter().copied().collect();
        let mut result = HashSet::new();

        for &op_idx in subgraph_ops {
            for &t in &problem.ops[op_idx].outputs {
                let all_consumers_inside = self.tensor_consumers[t]
                    .iter()
                    .all(|c| op_set.contains(c));
                let has_consumers = !self.tensor_consumers[t].is_empty();
                if has_consumers && all_consumers_inside && !self.graph_outputs.contains(&t) {
                    result.insert(t);
                }
            }
        }
        result
    }

    /// Determine the output tensor dimension (W_out, H_out) for a subgraph.
    /// For a valid subgraph, all boundary outputs share the same spatial dimensions.
    /// We use the first boundary output tensor's dimensions.
    pub fn output_dimensions(
        &self,
        problem: &Problem,
        subgraph_ops: &[usize],
    ) -> (i64, i64) {
        let boundary_out = self.boundary_outputs(problem, subgraph_ops);
        if boundary_out.is_empty() {
            // Fallback: use output of first op
            let first_op = subgraph_ops[0];
            let out_t = problem.ops[first_op].outputs[0];
            return (problem.tensors[out_t].width, problem.tensors[out_t].height);
        }
        let t = boundary_out[0];
        (problem.tensors[t].width, problem.tensors[t].height)
    }
}
