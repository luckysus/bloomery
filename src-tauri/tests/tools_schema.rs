use bloomery::agent::protocol::PermissionRisk;
use bloomery::agent::runtime::{
    CancellationToken, CompositeToolExecutor, ToolExecutor, ToolFuture, ToolHandler,
    ToolInvocation, ToolRegistration,
};
use bloomery::agent::tool_repair::ToolSpec;
use bloomery::steel::{OptimizationGateway, SteelToolExecutor};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Default)]
struct NoopGateway;

impl OptimizationGateway for NoopGateway {
    fn submit(&self, _arguments: Value) -> Result<Value, String> {
        Err("not used in schema tests".to_string())
    }

    fn status(&self, _task_id: &str) -> Result<Value, String> {
        Err("not used in schema tests".to_string())
    }
}

struct StaticHandler;

impl ToolHandler for StaticHandler {
    fn execute(&self, _arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async { Ok(json!({})) })
    }
}

struct OneTool {
    registration: ToolRegistration,
}

impl ToolExecutor for OneTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

fn tool(id: &str, schema: Value, risk: PermissionRisk) -> OneTool {
    OneTool {
        registration: ToolRegistration::new(
            ToolSpec {
                id: id.to_string(),
                name: id.rsplit('.').next().unwrap_or(id).to_string(),
                input_schema: schema,
                risk,
            },
            true,
            Arc::new(StaticHandler),
        ),
    }
}

#[test]
fn builtin_steel_tools_share_the_typed_schema_contract() {
    let steel = SteelToolExecutor::with_optimization_gateway(Arc::new(NoopGateway), true);
    assert_eq!(steel.registrations().len(), 3);
    for registration in steel.registrations() {
        registration
            .spec
            .validate_schema()
            .unwrap_or_else(|error| panic!("built-in tool schema must validate: {error}"));
    }
    let optimize = steel
        .registrations()
        .iter()
        .find(|registration| registration.spec.id == "steel.optimize_constrained")
        .expect("optimization tool registered");
    assert_eq!(optimize.spec.risk, PermissionRisk::ConfirmationRequired);
    let status = steel
        .registrations()
        .iter()
        .find(|registration| registration.spec.id == "steel.optimization_status")
        .expect("status tool registered");
    assert_eq!(status.spec.risk, PermissionRisk::Automatic);
}

#[test]
fn composite_executor_accepts_builtin_and_mcp_tools_with_valid_schemas() {
    let steel = SteelToolExecutor::with_optimization_gateway(Arc::new(NoopGateway), true);
    let mcp = tool(
        "mcp.standards.lookup",
        json!({"type": "object", "properties": {"grade": {"type": "string"}}}),
        PermissionRisk::ConfirmationRequired,
    );
    let composite =
        CompositeToolExecutor::try_new(vec![&steel, &mcp]).expect("valid schemas must combine");
    assert_eq!(composite.registrations().len(), 4);
}

#[test]
fn composite_executor_rejects_schemas_without_object_type() {
    let invalid = tool(
        "mcp.bad.schema",
        json!({"type": "string"}),
        PermissionRisk::Automatic,
    );
    let error = match CompositeToolExecutor::try_new(vec![&invalid]) {
        Ok(_) => panic!("non-object schema must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("must declare"), "unexpected error: {error}");
}

#[test]
fn composite_executor_rejects_malformed_properties_and_required() {
    let bad_properties = tool(
        "mcp.bad.properties",
        json!({"type": "object", "properties": ["nope"]}),
        PermissionRisk::Automatic,
    );
    let error = match CompositeToolExecutor::try_new(vec![&bad_properties]) {
        Ok(_) => panic!("non-object properties must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("properties"), "unexpected error: {error}");

    let bad_required = tool(
        "mcp.bad.required",
        json!({"type": "object", "required": [1]}),
        PermissionRisk::Automatic,
    );
    let error = match CompositeToolExecutor::try_new(vec![&bad_required]) {
        Ok(_) => panic!("non-string required entries must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("required"), "unexpected error: {error}");
}
