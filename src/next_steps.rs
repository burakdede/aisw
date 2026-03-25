use crate::types::Tool;

pub fn after_init() -> &'static str {
    "Next: run 'aisw list' to review profiles, then 'aisw use <tool> <name>' to switch."
}

pub fn after_add(tool: Tool, profile_name: &str, set_active: bool) -> String {
    if set_active {
        "Next: run 'aisw status' to confirm the current state.".to_owned()
    } else {
        format!(
            "Next: run 'aisw use {} {}' to activate it.",
            tool, profile_name
        )
    }
}

pub fn after_use() -> &'static str {
    "Next: run 'aisw status' to confirm the current state."
}

pub fn after_restore(tool: Tool, profile_name: &str) -> String {
    format!(
        "Next: run 'aisw use {} {}' to switch to it.",
        tool, profile_name
    )
}
