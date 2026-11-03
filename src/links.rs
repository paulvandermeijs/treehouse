//! Which links Treehouse opens.
//!
//! Links in the chat come from the model, and the text of a Markdown link can
//! hide its target. Handing every URL to the system would let a reply open a
//! script Pi just wrote (`file:///…/run.command` on macOS) or start another
//! app through its scheme, so only web and mail links open straight away, and
//! only when their text doesn't name a different site.

use std::path::{Path, PathBuf};

use markdown::mdast::Node;
use url::{Position, Url};

/// What to do with a clicked link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// A web or mail link, safe to hand to the browser or mail app: the
    /// URL as parsed, which is what was checked, rather than the text as
    /// written, which the system might read differently.
    Open(String),
    /// Only with the user's say-so, showing the full target.
    Ask(Ask),
    /// Never opens. `scheme` is the lower-cased scheme for the message, or
    /// `None` when the link isn't an absolute URL at all.
    Refuse { scheme: Option<String> },
}

/// Why a link asks first. Each `url` is the parsed URL, the one to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A file on this computer, which may run when opened.
    File { url: String, path: PathBuf },
    /// Another app's scheme (`vscode:`, `ftp:` …), which starts whatever
    /// app handles it.
    App { url: String, scheme: String },
    /// A web link whose text names a different site than it opens.
    Elsewhere {
        url: String,
        /// The link's text, as it reads.
        named: String,
    },
}

/// Schemes that run code or carry their own content, whatever the user says.
const REFUSED: [&str; 3] = ["javascript", "data", "vbscript"];

/// Classifies a link's target. `texts` are what the link reads as in the
/// message; a web link asks first when any of them names another site.
/// Relative links have nothing to resolve against, so they are refused like
/// anything else that doesn't parse.
pub fn classify(url: &str, texts: &[String]) -> LinkAction {
    // `Url::parse` lower-cases the scheme and drops tabs and newlines inside
    // it, so "JavaScript:" and "java\tscript:" can't slip through.
    let Ok(parsed) = Url::parse(url.trim()) else {
        return LinkAction::Refuse { scheme: None };
    };
    match parsed.scheme() {
        "http" | "https" => {
            let host = parsed.host_str().unwrap_or_default();
            let named = texts
                .iter()
                .find(|text| named_host(text).is_some_and(|named| !same_site(&named, host)));
            match named {
                Some(text) => LinkAction::Ask(Ask::Elsewhere {
                    url: parsed.to_string(),
                    named: text.trim().to_string(),
                }),
                None => LinkAction::Open(parsed.to_string()),
            }
        }
        "mailto" => LinkAction::Open(parsed.to_string()),
        scheme if REFUSED.contains(&scheme) => LinkAction::Refuse {
            scheme: Some(scheme.to_string()),
        },
        "file" => match parsed.to_file_path() {
            Ok(path) => LinkAction::Ask(Ask::File {
                url: parsed.to_string(),
                path,
            }),
            // A file on another machine (`file://server/x`) still goes to
            // whatever handles `file:`.
            Err(()) => LinkAction::Ask(Ask::App {
                url: parsed.to_string(),
                scheme: "file".into(),
            }),
        },
        scheme => LinkAction::Ask(Ask::App {
            url: parsed.to_string(),
            scheme: scheme.to_string(),
        }),
    }
}

/// The text of every link in `markdown` that points at `url`, as the chat
/// shows it. The text view only hands its click handler the URL, so the
/// text is read back from the message.
pub fn link_texts(markdown: &str, url: &str) -> Vec<String> {
    // The options the kit parses with, so the same links come out.
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.math_text = true;
    options.constructs.math_flow = true;
    let Ok(root) = markdown::to_mdast(markdown, &options) else {
        return Vec::new();
    };
    let mut definitions = Vec::new();
    collect_definitions(&root, &mut definitions);
    let mut texts = Vec::new();
    collect_link_texts(&root, url, &definitions, &mut texts);
    texts
}

fn collect_definitions(node: &Node, out: &mut Vec<(String, String)>) {
    if let Node::Definition(def) = node {
        out.push((def.identifier.clone(), def.url.clone()));
    }
    for child in node.children().into_iter().flatten() {
        collect_definitions(child, out);
    }
}

fn collect_link_texts(node: &Node, url: &str, defs: &[(String, String)], out: &mut Vec<String>) {
    let target = match node {
        Node::Link(link) => Some(link.url.as_str()),
        Node::LinkReference(link) => defs
            .iter()
            .find(|(id, _)| *id == link.identifier)
            .map(|(_, url)| url.as_str()),
        _ => None,
    };
    if target.is_some_and(|target| target.trim() == url.trim()) {
        out.push(node.to_string());
    }
    for child in node.children().into_iter().flatten() {
        collect_link_texts(child, url, defs, out);
    }
}

/// The host a link's text names, when the text reads as a web address or a
/// domain (`https://…`, `www.x.org`, `github.com/login`). Plain words, file
/// names and paths name nothing.
pub fn named_host(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Url::parse(text).ok()?.host_str().map(str::to_string);
    }
    let host = text.split(['/', '?', '#']).next()?;
    if host.contains('@') {
        return None;
    }
    // Normalised as a browser would: lower case, punycode, port dropped.
    let parsed = Url::parse(&format!("https://{host}/")).ok()?;
    let Some(url::Host::Domain(domain)) = parsed.host() else {
        return None;
    };
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|label| label.is_empty()) {
        return None;
    }
    let tld = labels[labels.len() - 1];
    (domain.starts_with("www.") || is_web_tld(tld)).then(|| domain.to_string())
}

/// Top-level domains that read as a site. Two-letter country domains count,
/// except the ones that are also common file extensions, so `main.rs` or
/// `README.md` isn't taken for a site; longer ones only when listed, since
/// `Cargo.toml` and `notes.txt` aren't sites either.
fn is_web_tld(tld: &str) -> bool {
    const FILE_LIKE: [&str; 14] = [
        "md", "rs", "py", "sh", "ts", "pl", "rb", "so", "cc", "mk", "ml", "ps", "pm", "gd",
    ];
    const GENERIC: [&str; 30] = [
        "com", "org", "net", "edu", "gov", "mil", "int", "info", "biz", "dev", "app", "xyz",
        "tech", "cloud", "site", "online", "page", "blog", "shop", "store", "news", "wiki", "link",
        "live", "pro", "name", "art", "email", "website", "space",
    ];
    if !tld.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    match tld.len() {
        2 => !FILE_LIKE.contains(&tld),
        _ => GENERIC.contains(&tld),
    }
}

/// Whether two hosts are the same site: equal, or equal once a leading
/// `www.` is dropped.
fn same_site(a: &str, b: &str) -> bool {
    let strip = |host: &str| {
        let host = host.to_ascii_lowercase();
        host.strip_prefix("www.")
            .map(str::to_string)
            .unwrap_or(host)
    };
    strip(a) == strip(b)
}

/// A URL in three parts around its host, so the host can be highlighted.
pub fn split_host(url: &str) -> Option<(String, String, String)> {
    let parsed = Url::parse(url).ok()?;
    parsed.host_str()?;
    Some((
        parsed[..Position::BeforeHost].to_string(),
        parsed[Position::BeforeHost..Position::AfterHost].to_string(),
        parsed[Position::AfterHost..].to_string(),
    ))
}

/// The extension of a file that runs, or installs something, when it's
/// opened, such as `command` for a macOS `.command` script.
pub fn program_extension(path: &Path) -> Option<String> {
    const PROGRAMS: [&str; 9] = [
        "command", "app", "sh", "exe", "bat", "ps1", "jar", "pkg", "dmg",
    ];
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    PROGRAMS.contains(&ext.as_str()).then_some(ext)
}

/// Whether a file on disk is marked executable, so the system may run it
/// rather than show it.
pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
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
    use super::{
        Ask, LinkAction, classify, is_safe_sign_in_url, link_texts, named_host, program_extension,
        split_host,
    };
    use std::path::{Path, PathBuf};

    fn refused(scheme: &str) -> LinkAction {
        LinkAction::Refuse {
            scheme: Some(scheme.to_string()),
        }
    }

    fn app(url: &str, scheme: &str) -> LinkAction {
        LinkAction::Ask(Ask::App {
            url: url.into(),
            scheme: scheme.into(),
        })
    }

    fn with_text(url: &str, text: &str) -> LinkAction {
        classify(url, &[text.to_string()])
    }

    #[test]
    fn opens_web_and_mail_links() {
        assert!(matches!(
            classify("https://example.com/a?b=c#d", &[]),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("http://localhost:3000", &[]),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("HTTPS://EXAMPLE.COM", &[]),
            LinkAction::Open(_)
        ));
        assert_eq!(
            classify("  https://example.com \n", &[]),
            LinkAction::Open("https://example.com/".into())
        );
        // What opens is what was checked: tabs and newlines the parser drops
        // don't reach the system.
        assert_eq!(
            classify("ht\ttps://example.com/a", &[]),
            LinkAction::Open("https://example.com/a".into())
        );
        assert!(matches!(
            classify("mailto:someone@example.com", &[]),
            LinkAction::Open(_)
        ));
        assert!(matches!(
            classify("MailTo:someone@example.com", &[]),
            LinkAction::Open(_)
        ));
        assert!(matches!(classify("mailto:", &[]), LinkAction::Open(_)));
    }

    #[test]
    fn asks_for_files() {
        assert_eq!(
            classify("file:///Users/x/run.command", &[]),
            LinkAction::Ask(Ask::File {
                url: "file:///Users/x/run.command".into(),
                path: PathBuf::from("/Users/x/run.command"),
            })
        );
        assert!(matches!(
            classify("FILE:///etc/passwd", &[]),
            LinkAction::Ask(Ask::File { .. })
        ));
        assert_eq!(
            classify("file://server/share/x", &[]),
            app("file://server/share/x", "file")
        );
    }

    #[test]
    fn asks_for_other_schemes() {
        assert_eq!(
            classify("vscode://file/Users/x/project/src/main.rs:12", &[]),
            app("vscode://file/Users/x/project/src/main.rs:12", "vscode")
        );
        assert_eq!(
            classify(
                "x-apple.systempreferences:com.apple.preference.security",
                &[]
            ),
            app(
                "x-apple.systempreferences:com.apple.preference.security",
                "x-apple.systempreferences"
            )
        );
        assert_eq!(
            classify("ftp://example.com/file", &[]),
            app("ftp://example.com/file", "ftp")
        );
        assert_eq!(classify("ssh://host", &[]), app("ssh://host", "ssh"));
        // A look-alike of a web scheme is a different scheme.
        assert_eq!(
            classify("https+x://example.com", &[]),
            app("https+x://example.com", "https+x")
        );
    }

    #[test]
    fn refuses_script_and_data_links() {
        assert_eq!(classify("javascript:alert(1)", &[]), refused("javascript"));
        assert_eq!(classify("JavaScript:alert(1)", &[]), refused("javascript"));
        assert_eq!(
            classify("  javascript:alert(1)", &[]),
            refused("javascript")
        );
        assert_eq!(
            classify("java\tscript:alert(1)", &[]),
            refused("javascript")
        );
        assert_eq!(
            classify("java\nscript:alert(1)", &[]),
            refused("javascript")
        );
        assert_eq!(
            classify("data:text/html,<script>alert(1)</script>", &[]),
            refused("data")
        );
        assert_eq!(classify("DATA:text/plain,hi", &[]), refused("data"));
        assert_eq!(classify("vbscript:msgbox(1)", &[]), refused("vbscript"));
        // A script link stays refused whatever its text says.
        assert_eq!(
            with_text("javascript:alert(1)", "github.com"),
            refused("javascript")
        );
    }

    #[test]
    fn refuses_links_without_a_scheme() {
        let none = LinkAction::Refuse { scheme: None };
        assert_eq!(classify("", &[]), none);
        assert_eq!(classify("   ", &[]), none);
        assert_eq!(classify("docs/x.md", &[]), none);
        assert_eq!(classify("./run.command", &[]), none);
        assert_eq!(classify("/etc/passwd", &[]), none);
        assert_eq!(classify("#heading", &[]), none);
        assert_eq!(classify("//example.com/x", &[]), none);
        assert_eq!(classify(":nothing", &[]), none);
        assert_eq!(classify("1http://example.com", &[]), none);
    }

    #[test]
    fn text_naming_another_site_asks() {
        assert_eq!(
            with_text(
                "https://github-login.example.net/session",
                "github.com/login"
            ),
            LinkAction::Ask(Ask::Elsewhere {
                url: "https://github-login.example.net/session".into(),
                named: "github.com/login".into(),
            })
        );
        assert!(matches!(
            with_text("https://evil.example/", "https://github.com/login"),
            LinkAction::Ask(Ask::Elsewhere { .. })
        ));
        assert!(matches!(
            with_text("http://evil.example/", "www.python.org"),
            LinkAction::Ask(Ask::Elsewhere { .. })
        ));
        // Userinfo before the host doesn't make it the named site.
        assert!(matches!(
            with_text("https://github.com@evil.example/", "github.com"),
            LinkAction::Ask(Ask::Elsewhere { .. })
        ));
        // A subdomain is another host.
        assert!(matches!(
            with_text("https://docs.github.com/", "github.com"),
            LinkAction::Ask(Ask::Elsewhere { .. })
        ));
        // Any of the texts naming another site is enough.
        assert!(matches!(
            classify(
                "https://evil.example/",
                &["the page".into(), "github.com".into()]
            ),
            LinkAction::Ask(Ask::Elsewhere { .. })
        ));
    }

    #[test]
    fn text_naming_the_same_site_opens() {
        let opens = |url: &str, text: &str| matches!(with_text(url, text), LinkAction::Open(_));
        assert!(opens("https://github.com/login", "github.com/login"));
        assert!(opens("https://www.github.com/x", "github.com"));
        assert!(opens("https://github.com/x", "www.github.com"));
        assert!(opens("https://GitHub.com/x", "github.COM/y"));
        assert!(opens("https://github.com/a/b", "https://github.com/c"));
        assert!(opens("https://example.com:8443/", "example.com:8443"));
        // A bare URL autolinked by GFM reads as itself.
        assert!(opens("https://socket.io/docs", "https://socket.io/docs"));
        // Plain words and file names don't compare anything.
        assert!(opens("https://github.com/x/releases", "the releases page"));
        assert!(opens(
            "https://github.com/x/blob/main/src/main.rs",
            "main.rs"
        ));
        assert!(opens(
            "https://github.com/x/blob/main/README.md",
            "README.md"
        ));
        assert!(opens(
            "https://github.com/x/blob/main/Cargo.toml",
            "Cargo.toml"
        ));
        assert!(opens(
            "https://github.com/x/blob/main/src/a.rs",
            "src/approval.rs"
        ));
        assert!(opens("https://crates.io/crates/url", "v2.5.8"));
        assert!(opens("https://example.com/", "someone@example.org"));
    }

    #[test]
    fn names_hosts_only_for_addresses_and_domains() {
        assert_eq!(
            named_host("github.com/login").as_deref(),
            Some("github.com")
        );
        assert_eq!(named_host(" https://x.org/a ").as_deref(), Some("x.org"));
        assert_eq!(named_host("www.x.org").as_deref(), Some("www.x.org"));
        assert_eq!(
            named_host("www.example.toml").as_deref(),
            Some("www.example.toml")
        );
        assert_eq!(named_host("anthropic.ai").as_deref(), Some("anthropic.ai"));
        assert_eq!(named_host("Example.NL").as_deref(), Some("example.nl"));
        assert_eq!(
            named_host("example.com?q=1").as_deref(),
            Some("example.com")
        );
        assert_eq!(named_host("the releases page"), None);
        assert_eq!(named_host("releases"), None);
        assert_eq!(named_host("main.rs"), None);
        assert_eq!(named_host("notes.txt"), None);
        assert_eq!(named_host("1.2.3"), None);
        assert_eq!(named_host("127.0.0.1"), None);
        assert_eq!(named_host("e.g."), None);
        assert_eq!(named_host(""), None);
    }

    #[test]
    fn reads_link_texts_from_the_message() {
        let message = "Sign in at [github.com/login](https://evil.example/s), or see \
                       [the docs][docs] and <https://example.com/a>.\n\n\
                       Also [**bold** `code`](https://evil.example/s).\n\n\
                       [docs]: https://docs.example.com/";
        assert_eq!(
            link_texts(message, "https://evil.example/s"),
            vec!["github.com/login".to_string(), "bold code".to_string()]
        );
        assert_eq!(
            link_texts(message, "https://docs.example.com/"),
            vec!["the docs".to_string()]
        );
        assert_eq!(
            link_texts(message, "https://example.com/a"),
            vec!["https://example.com/a".to_string()]
        );
        assert!(link_texts(message, "https://other.example/").is_empty());
    }

    #[test]
    fn spots_program_files_by_extension() {
        let ext = |path: &str| program_extension(Path::new(path));
        assert_eq!(ext("/tmp/run.command").as_deref(), Some("command"));
        assert_eq!(ext("/Applications/X.app").as_deref(), Some("app"));
        assert_eq!(ext("/tmp/SETUP.EXE").as_deref(), Some("exe"));
        for program in ["a.sh", "a.bat", "a.ps1", "a.jar", "a.pkg", "a.dmg"] {
            assert!(ext(program).is_some(), "{program}");
        }
        assert_eq!(ext("/tmp/notes.md"), None);
        assert_eq!(ext("/tmp/Makefile"), None);
        assert_eq!(ext("/tmp/command"), None);
    }

    #[test]
    fn splits_a_url_around_its_host() {
        assert_eq!(
            split_host("https://github-login.example.net/session"),
            Some((
                "https://".into(),
                "github-login.example.net".into(),
                "/session".into()
            ))
        );
        assert_eq!(
            split_host("https://github.com@evil.example:8080/x"),
            Some((
                "https://github.com@".into(),
                "evil.example".into(),
                ":8080/x".into()
            ))
        );
        assert_eq!(split_host("mailto:x@example.com"), None);
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
