/*
 * C4 Model for MLSys DAG Scheduler
 *
 * This is NOT a web service. It is a CLI tool that reads a problem JSON
 * and produces an optimized execution schedule JSON.
 *
 * Since Structurizr DSL is designed for distributed systems, we use it
 * here to document module-level architecture within a single process.
 */

workspace "MLSys DAG Scheduler" "Computational graph scheduler for memory-constrained AI accelerators. Track A: Rust binary. Track B: Python agent." {

    model {
        user = person "User" "Contest participant or researcher running the scheduler"
        evaluator = softwareSystem "C++ Evaluator" "Reference Evaluate() in mlsys.h that scores solutions" "External"

        scheduler = softwareSystem "MLSys Scheduler (Track A)" "Rust CLI binary" {

            cli = container "CLI" "Entry point" "Rust" {
                description "Reads problem JSON, invokes optimizer pipeline, writes solution JSON. Subcommands: solve (default) and evaluate."
            }

            parser = container "Parser" "JSON -> Problem" "Rust" {
                description "Deserializes problem JSON into typed Problem struct using serde_json"
            }

            serializer = container "Serializer" "Solution -> JSON" "Rust" {
                description "Serializes Solution struct into output JSON using serde_json"
            }

            dagModule = container "DAG Module" "Graph analysis" "Rust" {
                description "Topological sort (Kahn's), adjacency lists, graph input/output identification"
            }

            latencyModel = container "Latency Model" "Roofline calculator" "Rust" {
                description "Computes per-step and per-subgraph latency using roofline model"
            }

            memoryModel = container "Memory Model" "Working-set calculator" "Rust" {
                description "Computes working-set size, checks OOM constraints"
            }

            baseline = container "Baseline Scheduler" "Naive schedule" "Rust" {
                description "One op per subgraph, native granularity, no retention"
            }

            optimizerPipeline = container "Optimizer Pipeline" "Schedule refinement" "Rust" {
                description "Orchestrates 9 optimizer stages: baseline, fusion, retention x2, split-K, granularity search, emergency OOM fix, latency recalculation, traversal optimization"

                fusionComponent = component "Fusion" "Greedy chain fusion" {
                    description "Merges adjacent ops into subgraphs where working set fits"
                }
                retentionComponent = component "Retention" "Tensor retention" {
                    description "Decides which output tensors to keep in fast memory across subgraph boundaries"
                }
                splitKComponent = component "Split-K" "Reduction splitting" {
                    description "Finds optimal k for MatMul subgraphs under memory pressure"
                }
                granularityComponent = component "Granularity Search" "Spatial tiling" {
                    description "Searches (w, h, k) candidates to minimize per-subgraph latency"
                }
                traversalComponent = component "Traversal Order" "Tile ordering" {
                    description "Snake/zig-zag traversal to reduce input strip reloads"
                }
            }

            /* Relationships within the system */
            cli -> parser "reads problem JSON via"
            cli -> optimizerPipeline "invokes"
            cli -> serializer "writes solution JSON via"

            optimizerPipeline -> baseline "starts from"
            optimizerPipeline -> dagModule "queries DAG structure"
            optimizerPipeline -> latencyModel "evaluates latency"
            optimizerPipeline -> memoryModel "checks OOM"

            baseline -> dagModule "gets topological order from"
            baseline -> latencyModel "calculates latency via"
            baseline -> memoryModel "checks working set via"

            fusionComponent -> memoryModel "validates merged working set"
            fusionComponent -> latencyModel "compares latency before/after"
            retentionComponent -> memoryModel "checks residual capacity"
            splitKComponent -> memoryModel "finds largest fitting k"
            splitKComponent -> latencyModel "evaluates split-K latency"
            granularityComponent -> memoryModel "checks OOM for each candidate"
            granularityComponent -> latencyModel "evaluates candidate latency"
        }

        /* External relationships */
        user -> cli "runs"
        user -> evaluator "validates solution with"
        serializer -> evaluator "solution JSON consumed by"
    }

    views {
        systemContext scheduler "SystemContext" {
            include *
            autolayout lr
        }

        container scheduler "Containers" {
            include *
            autolayout tb
        }

        component optimizerPipeline "OptimizerComponents" {
            include *
            autolayout lr
        }
    }
}
