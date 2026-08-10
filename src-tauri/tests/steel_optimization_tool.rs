use bloomery::agent::protocol::PermissionRisk;
use bloomery::agent::runtime::{CancellationToken, ToolExecutor, ToolInvocation};
use bloomery::steel::{OptimizationGateway, SteelToolExecutor};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Default)]
struct FakeGateway {
    submitted: Mutex<Vec<Value>>,
    fail_submit: bool,
}

impl OptimizationGateway for FakeGateway {
    fn submit(&self, arguments: Value) -> Result<Value, String> {
        if self.fail_submit {
            return Err("training task was not found in the local workspace".to_string());
        }
        self.submitted.lock().expect("submit lock").push(arguments);
        Ok(json!({
            "id": "0d7f0f8a-7ba5-4f1b-9a5a-111111111111",
            "kind": "compute_optimize_constrained",
            "state": "queued",
            "progress": 0,
        }))
    }

    fn status(&self, task_id: &str) -> Result<Value, String> {
        if task_id == "missing" {
            return Err("task_not_found: task not found".to_string());
        }
        Ok(json!({
            "task_id": task_id,
            "state": "completed",
            "progress": 100,
            "result": {
                "method": "tpe",
                "recommendations": [{"values": {"temperature": 4.0}, "feasible": true}]
            }
        }))
    }
}

fn valid_arguments() -> Value {
    json!({
        "datasetId": "dataset-1",
        "trainingTaskId": "0d7f0f8a-7ba5-4f1b-9a5a-222222222222",
        "direction": "minimize",
        "objectiveColumns": [0],
        "bounds": [{"min": 0.0, "max": 10.0}],
        "fixedValues": [null],
        "constraints": [{
            "kind": "inequality",
            "coefficients": [1.0],
            "value": 4.0,
            "tolerance": 0.0
        }],
        "trials": 24,
        "seed": 7
    })
}

async fn invoke(
    executor: &SteelToolExecutor,
    tool_id: &str,
    tool_name: &str,
    arguments: Value,
    cancelled: bool,
) -> Result<Value, bloomery::agent::runtime::ToolExecutionError> {
    executor
        .execute(
            ToolInvocation {
                tool_call_id: Uuid::new_v4(),
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                arguments,
            },
            CancellationToken::new(move || cancelled),
        )
        .await
}

#[test]
fn gateway_executor_registers_optimization_tools_with_confirmation_risk() {
    let executor = SteelToolExecutor::with_optimization_gateway(Arc::new(FakeGateway::default()), true);
    let registrations = executor.registrations();
    assert_eq!(registrations.len(), 3);
    let optimize = registrations
        .iter()
        .find(|registration| registration.spec.id == "steel.optimize_constrained")
        .expect("optimization tool must be registered");
    assert_eq!(optimize.spec.risk, PermissionRisk::ConfirmationRequired);
    let status = registrations
        .iter()
        .find(|registration| registration.spec.id == "steel.optimization_status")
        .expect("status tool must be registered");
    assert_eq!(status.spec.risk, PermissionRisk::Automatic);
    assert!(
        SteelToolExecutor::with_optimization_gateway(Arc::new(FakeGateway::default()), false)
            .registrations()
            .is_empty(),
        "limited models must not receive optimization tools"
    );
}

#[tokio::test]
async fn optimization_tool_submits_a_validated_task_through_the_gateway() {
    let gateway = Arc::new(FakeGateway::default());
    let executor = SteelToolExecutor::with_optimization_gateway(gateway.clone(), true);

    let result = invoke(
        &executor,
        "steel.optimize_constrained",
        "optimize_constrained",
        valid_arguments(),
        false,
    )
    .await
    .expect("valid optimization request must be submitted");

    assert_eq!(result["kind"], "compute_optimize_constrained");
    assert_eq!(result["state"], "queued");
    let submitted = gateway.submitted.lock().expect("submit lock");
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0]["datasetId"], "dataset-1");
    assert_eq!(submitted[0]["constraints"][0]["kind"], "inequality");
    assert_eq!(submitted[0]["trials"], json!(24));
}

#[tokio::test]
async fn optimization_tool_rejects_invalid_arguments_before_submission() {
    let gateway = Arc::new(FakeGateway::default());
    let executor = SteelToolExecutor::with_optimization_gateway(gateway.clone(), true);

    let mut arguments = valid_arguments();
    arguments["direction"] = json!("sideways");
    let error = invoke(
        &executor,
        "steel.optimize_constrained",
        "optimize_constrained",
        arguments,
        false,
    )
    .await
    .expect_err("invalid direction must be rejected");
    assert_eq!(error.code, "steel_optimization_invalid");

    let mut arguments = valid_arguments();
    arguments["trials"] = json!(10_000);
    let error = invoke(
        &executor,
        "steel.optimize_constrained",
        "optimize_constrained",
        arguments,
        false,
    )
    .await
    .expect_err("oversized trials must be rejected");
    assert_eq!(error.code, "steel_optimization_invalid");

    assert!(gateway.submitted.lock().expect("submit lock").is_empty());
}

#[tokio::test]
async fn optimization_tool_surfaces_gateway_failures_as_typed_errors() {
    let gateway = Arc::new(FakeGateway {
        fail_submit: true,
        ..Default::default()
    });
    let executor = SteelToolExecutor::with_optimization_gateway(gateway, true);

    let error = invoke(
        &executor,
        "steel.optimize_constrained",
        "optimize_constrained",
        valid_arguments(),
        false,
    )
    .await
    .expect_err("gateway failure must surface");
    assert_eq!(error.code, "steel_optimization_submit_failed");
}

#[tokio::test]
async fn optimization_tools_respect_cancellation() {
    let executor = SteelToolExecutor::with_optimization_gateway(Arc::new(FakeGateway::default()), true);

    let error = invoke(
        &executor,
        "steel.optimize_constrained",
        "optimize_constrained",
        valid_arguments(),
        true,
    )
    .await
    .expect_err("cancelled submissions must be rejected");
    assert_eq!(error.code, "steel_optimization_cancelled");
}

#[tokio::test]
async fn optimization_status_tool_reports_result_and_errors() {
    let executor = SteelToolExecutor::with_optimization_gateway(Arc::new(FakeGateway::default()), true);

    let status = invoke(
        &executor,
        "steel.optimization_status",
        "optimization_status",
        json!({"task_id": "0d7f0f8a-7ba5-4f1b-9a5a-111111111111"}),
        false,
    )
    .await
    .expect("status lookup must succeed");
    assert_eq!(status["state"], "completed");
    assert_eq!(status["result"]["method"], "tpe");
    assert_eq!(status["result"]["recommendations"][0]["feasible"], json!(true));

    let error = invoke(
        &executor,
        "steel.optimization_status",
        "optimization_status",
        json!({"task_id": "missing"}),
        false,
    )
    .await
    .expect_err("missing tasks must surface an error");
    assert_eq!(error.code, "steel_optimization_status_failed");
}
