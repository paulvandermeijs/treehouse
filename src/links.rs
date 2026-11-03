//! Which links Treehouse opens.
//!
//! Links in the chat come from the model, and the text of a Markdown link can
//! hide its target. Handing every URL to the system would let a reply open a
//! script Pi just wrote (`file:///…/run.command` on macOS) or start another
//! app through its scheme, so only web and mail links open straight away.

use url::Url;

/// What to do with a clicked link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// A web or mail link, safe to hand to the browser or mail app: the
    /// URL as parsed, which is what was checked, rather than the text as
    /// written, which the system might read differently.
    Open(String),
    /// Some other scheme (`file:`, `vscode:`, `ftp:` …): only with the
    /// user's say-so, and showing the full target.
    Confirm,
    /// Never opens. `scheme` is the lower-cased scheme for the message, or
    /// `None` when the link isn't an absolute URL at all.
    Refuse { scheme: Option<String> },
}

/// Schemes that run code or carry their own content, whatever the user says.
const REFUSED: [&str; 3] = ["javascript", "data", "vbscript"];

/// Classifies a link's target. Relative links have nothing to resolve
/// against, so they are refused like anything else that doesn't parse.
pub fn classify(url: &str) -> LinkAction {
    // `Url::parse` lower-cases the scheme and drops tabs and newlines inside
    // it, so "JavaScript:" and "java\tscript:" can't slip through.
    let Ok(parsed) = Url::parse(url.trim()) else {
        return LinkAction::Refuse { scheme: None };
    };
    match parsed.scheme() {
        "http" | "https" | "mailto" => LinkAction::Open(parsed.to_string()),
        scheme if REFUSED.contains(&scheme) => LinkAction::Refuse {
            scheme: Some(scheme.to_string()),
        },
        _ => LinkAction::Confirm,
    }
}

/// Whether the MCP sign-in button may open this page. It has to be https,
/// since the page asks for the user's credentials. Plain http is allowed
/// only on this machine (`localhost`, `127.0.0.1`, `[::1]`): a local MCP
/// server may serve its own sign-in page that way, and the traffic never
/// leaves the machine.
pub fn is_safe_sign_in_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url.trim()) else {
        return false;
    };
    match parsed.scheme() {
        "https" => parsed.host().is_some(),
        "http" => match parsed.host() {
            Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{LinkAction, classify, is_safe_sign_in_url};

    fn refused(scheme: &str) -> LinkAction {
        LinkAction::Refuse {
            scheme: Some(scheme.to_string()),
        }
    }

    #[test]
    fn opens_web_and_mail_links() {
        assert!(matches!(
            classify("https://example.com/a?b=c#d"),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("http://localhost:3000"),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("HTTPS://EXAMPLE.COM"),
            LinkAction::Open(_)
        ));
        assert_eq!(
            classify("  https://example.com \n"),
            LinkAction::Open("https://example.com/".into())
        );
        // What opens is what was checked: tabs and newlines the parser drops
        // don't reach the system.
        assert_eq!(
            classify("ht\ttps://example.com/a"),
            LinkAction::Open("https://example.com/a".into())
        );
        assert!(matches!(
            classify("mailto:someone@example.com"),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("MailTo:someone@example.com"),
            LinkAction::Open(_)
        ));
        assert!(matches!(classify("mailto:"), LinkAction::Open(_)));
    }

    #[test]
    fn confirms_other_schemes() {
        assert_eq!(classify("file:///Users/x/run.command"), LinkAction::Confirm);
        assert_eq!(classify("FILE:///etc/passwd"), LinkAction::Confirm);
        assert_eq!(
            classify("vscode://file/Users/x/project/src/main.rs:12"),
            LinkAction::Confirm
        );
        assert_eq!(
            classify("x-apple.systempreferences:com.apple.preference.security"),
            LinkAction::Confirm
        );
        assert_eq!(classify("ftp://example.com/file"), LinkAction::Confirm);
        assert_eq!(classify("ssh://host"), LinkAction::Confirm);
        // A look-alike of a web scheme is a different scheme.
        assert_eq!(classify("https+x://example.com"), LinkAction::Confirm);
    }

    #[test]
    fn refuses_script_and_data_links() {
        assert_eq!(classify("javascript:alert(1)"), refused("javascript"));
        assert_eq!(classify("JavaScript:alert(1)"), refused("javascript"));
        assert_eq!(classify("  javascript:alert(1)"), refused("javascript"));
        assert_eq!(classify("java\tscript:alert(1)"), refused("javascript"));
        assert_eq!(classify("java\nscript:alert(1)"), refused("javascript"));
        assert_eq!(
            classify("data:text/html,<script>alert(1)</script>"),
            refused("data")
        );
        assert_eq!(classify("DATA:text/plain,hi"), refused("data"));
        assert_eq!(classify("vbscript:msgbox(1)"), refused("vbscript"));
    }

    #[test]
    fn refuses_links_without_a_scheme() {
        let none = LinkAction::Refuse { scheme: None };
        assert_eq!(classify(""), none);
        assert_eq!(classify("   "), none);
        assert_eq!(classify("docs/x.md"), none);
        assert_eq!(classify("./run.command"), none);
        assert_eq!(classify("/etc/passwd"), none);
        assert_eq!(classify("#heading"), none);
        assert_eq!(classify("//example.com/x"), none);
        assert_eq!(classify(":nothing"), none);
        assert_eq!(classify("1http://example.com"), none);
    }

    #[test]
    fn sign_in_needs_https_or_this_machine() {
        assert!(is_safe_sign_in_url("https://www.figma.com/oauth?x=1"));
        assert!(is_safe_sign_in_url("HTTPS://auth.example.com/authorize"));
        assert!(is_safe_sign_in_url(" https://auth.example.com "));
        assert!(is_safe_sign_in_url("http://localhost:8080/authorize"));
        assert!(is_safe_sign_in_url("http://LOCALHOST/authorize"));
        assert!(is_safe_sign_in_url("http://127.0.0.1:3845/oauth"));
        assert!(is_safe_sign_in_url("http://[::1]:3845/oauth"));

        assert!(!is_safe_sign_in_url("http://auth.example.com/authorize"));
        assert!(!is_safe_sign_in_url("http://localhost.example.com/"));
        assert!(!is_safe_sign_in_url("http://10.0.0.5/"));
        assert!(!is_safe_sign_in_url("file:///Users/x/login.html"));
        assert!(!is_safe_sign_in_url("javascript:alert(1)"));
        assert!(!is_safe_sign_in_url("vscode://auth"));
        assert!(!is_safe_sign_in_url(""));
        assert!(!is_safe_sign_in_url("auth.example.com/authorize"));
    }
}
