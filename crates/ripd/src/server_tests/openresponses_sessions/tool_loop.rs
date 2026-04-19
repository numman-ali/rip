use super::*;

#[tokio::test]
async fn prompt_openresponses_executes_function_tools_and_sends_followup() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../../../fixtures/openresponses/tool_loop_apply_patch_first.sse"),
        false,
    )
    .await;
}

#[tokio::test]
async fn prompt_openresponses_executes_function_tools_with_argument_deltas() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../../../fixtures/openresponses/tool_loop_apply_patch_args_delta.sse"),
        false,
    )
    .await;
}

#[tokio::test]
async fn prompt_openresponses_executes_function_tools_stateless_history() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../../../fixtures/openresponses/tool_loop_apply_patch_first.sse"),
        true,
    )
    .await;
}
