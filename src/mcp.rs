//! Reads Pi's MCP sign-in messages.
//!
//! Pi's MCP support reports through RPC notifications written for people,
//! and signs in with `/mcp login <server>`. Treehouse turns those messages
//! into sign-in cards, so these parsers follow Pi's wording
//! (`extensions/mcp/index.ts`).

/// "MCP servers need attention:\n  figma: needs sign-in\nRun /mcp to fix."
/// Returns each listed server and its problem.
pub fn parse_attention(message: &str) -> Option<Vec<(String, String)>> {
    let rest = message.strip_prefix("MCP servers need attention:")?;
    Some(
        rest.lines()
            .filter_map(|line| {
                let line = line.trim();
                let (name, state) = line.split_once(": ")?;
                (name != "config" && !name.contains(' '))
                    .then(|| (name.to_string(), state.to_string()))
            })
            .collect(),
    )
}

/// "Sign in to MCP server \"figma\" in your browser:\nhttps://…"
pub fn parse_sign_in_url(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Sign in to MCP server \"")?;
    let (name, rest) = rest.split_once('"')?;
    let url = rest.lines().nth(1)?.trim();
    Some((name.to_string(), url.to_string()))
}

/// "Signed in to MCP server \"figma\" (12 tools)."
pub fn parse_signed_in(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Signed in to MCP server \"")?;
    let (name, rest) = rest.split_once('"')?;
    let detail = rest
        .trim()
        .trim_start_matches('(')
        .trim_end_matches('.')
        .trim_end_matches(')');
    Some((name.to_string(), detail.to_string()))
}

/// The title of the input request Pi sends while it waits for the browser:
/// "Waiting for sign-in to \"figma\". If the browser cannot reach this
/// machine, paste the URL it was redirected to."
pub fn parse_waiting(title: &str) -> Option<String> {
    let rest = title.strip_prefix("Waiting for sign-in to \"")?;
    Some(rest.split_once('"')?.0.to_string())
}

/// The message that starts a sign-in.
pub fn login_command(server: &str) -> String {
    format!("/mcp login {server}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pi_messages() {
        let attention = parse_attention(
            "MCP servers need attention:\n  config: bad entry\n  figma: needs sign-in\n  docs: failed: ECONNREFUSED\nRun /mcp to fix.",
        )
        .unwrap();
        assert_eq!(attention.len(), 2);
        assert_eq!(attention[0], ("figma".into(), "needs sign-in".into()));

        assert_eq!(
            parse_sign_in_url(
                "Sign in to MCP server \"figma\" in your browser:\nhttps://www.figma.com/oauth?x=1"
            ),
            Some(("figma".into(), "https://www.figma.com/oauth?x=1".into()))
        );
        assert_eq!(
            parse_signed_in("Signed in to MCP server \"figma\" (12 tools)."),
            Some(("figma".into(), "12 tools".into()))
        );
        assert_eq!(
            parse_waiting(
                "Waiting for sign-in to \"figma\". If the browser cannot reach this machine, paste the URL it was redirected to."
            ),
            Some("figma".into())
        );
        assert_eq!(parse_attention("Something else"), None);
    }
}
