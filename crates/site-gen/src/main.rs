//! Trunk bundles the assets first; this renders the pages into its staging directory before publication.
//! The download sizes are read from the latest release, so the page always shows what is really there.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const LATEST: &str = "https://api.github.com/repos/HsiangNianian/boardcast/releases/latest";

/// The latest release's packages, as file name and size in bytes.
type Assets = Vec<(String, u64)>;

fn parse_assets(release: &serde_json::Value) -> Assets {
    release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["name"].as_str()?.to_owned(), a["size"].as_u64()?)))
        .collect()
}

/// `trunk serve` runs this on every change, so a fetched release is kept for an hour.
fn cache_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/site-release.json")
}

fn fetch_release() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut request = ureq::get(LATEST)
        .timeout(Duration::from_secs(10))
        .set("Accept", "application/vnd.github+json");
    // CI passes its token, so the API's limit for anonymous callers never fails a build.
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    Ok(request.call()?.into_json()?)
}

/// The latest release's packages: fresh from the cache, else fetched. Offline, the page goes without sizes,
/// unless `BOARDCAST_REQUIRE_RELEASE` is set, as it is for the published site.
fn release_assets() -> Result<Assets, Box<dyn std::error::Error>> {
    let cache = cache_path();
    let fresh = fs::metadata(&cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(3600));
    if fresh {
        if let Ok(release) = fs::read_to_string(&cache)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
        {
            return Ok(parse_assets(&release));
        }
    }
    match fetch_release() {
        Ok(release) => {
            let _ = fs::write(&cache, release.to_string());
            let assets = parse_assets(&release);
            if assets.is_empty() && env::var_os("BOARDCAST_REQUIRE_RELEASE").is_some() {
                return Err("the latest release lists no packages".into());
            }
            Ok(assets)
        }
        Err(e) if env::var_os("BOARDCAST_REQUIRE_RELEASE").is_some() => Err(format!("reading the latest release: {e}").into()),
        Err(e) => {
            eprintln!("boardcast-site-gen: no download sizes, the latest release could not be read: {e}");
            Ok(Assets::new())
        }
    }
}

/// The size of a package, named as the page names it: `linux`, `macos-en`, and so on. Packages for
/// different processors of one system differ by a few megabytes, so the larger is shown.
fn package_size(assets: &Assets, key: &str) -> Option<u64> {
    let (system, variant) = key.split_once('-').map_or((key, ""), |(s, v)| (s, v));
    let ext = if system == "linux" { "tar.gz" } else { "zip" };
    let variant = if variant.is_empty() { String::new() } else { format!("-{variant}") };
    let ending = format!("-{system}{variant}.{ext}");
    assets.iter().filter(|(name, _)| name.starts_with("boardcast-") && name.ends_with(&ending)).map(|&(_, size)| size).max()
}

fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1000.0 {
        format!("{:.1} gb", mb / 1024.0)
    } else {
        format!("{} mb", mb.round().max(1.0))
    }
}

/// Fills each `{{ size:<package> }}` with the package's size, or leaves it empty when it is not known.
fn fill_sizes(content: &str, assets: &Assets) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("{{ size:") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find(" }}") else { break };
        let key = &rest[start + 8..start + len];
        out.push_str(&package_size(assets, key).map(human_size).unwrap_or_default());
        rest = &rest[start + len + 3..];
    }
    out.push_str(rest);
    out
}

struct Page {
    slug: &'static str,
    label: &'static str,
    title: &'static str,
    description: &'static str,
    content: &'static str,
}

const PAGES: [Page; 5] = [
    Page {
        slug: "",
        label: "home",
        title: "boardcast — talk, and the board draws itself",
        description: "A whiteboard that writes down your meeting, stroke by stroke. Speech recognition and the board model run on your own computer.",
        content: include_str!("../pages/home.html"),
    },
    Page {
        slug: "demo",
        label: "web demo",
        title: "boardcast — web demo",
        description: "Sample meetings in English, replayed with the board engine the boardcast app runs, stroke by stroke.",
        content: include_str!("../pages/demo.html"),
    },
    Page {
        slug: "faq",
        label: "faq",
        title: "FAQ — boardcast",
        description: "Questions about boardcast: what it does, what it needs, which package to get, and where your meetings stay.",
        content: include_str!("../pages/faq.html"),
    },
    Page {
        slug: "privacy",
        label: "privacy",
        title: "Privacy — boardcast",
        description: "What boardcast keeps, where, and the few times it uses the network.",
        content: include_str!("../pages/privacy.html"),
    },
    Page {
        slug: "terms",
        label: "terms",
        title: "Terms — boardcast",
        description: "The license boardcast is under, the licenses of the models and open-source work it is built on, and thanks to their authors.",
        content: include_str!("../pages/terms.html"),
    },
];

const DOCUMENT_LINKS: &[usize] = &[2, 3, 4];
const UPDATED: &str = "2026-11-10";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn root(page: &Page) -> &'static str {
    if page.slug.is_empty() {
        "./"
    } else {
        "../"
    }
}

fn links(page: &Page, items: &[usize]) -> String {
    items
        .iter()
        .map(|&i| {
            let target = &PAGES[i];
            let suffix = if target.slug.is_empty() {
                String::new()
            } else {
                format!("{}/", target.slug)
            };
            let current = if target.slug == page.slug {
                r#" class="on" aria-current="page""#
            } else {
                ""
            };
            format!(
                r#"<a href="{}{suffix}"{current}>{}</a>"#,
                root(page),
                target.label
            )
        })
        .collect()
}

fn navigation(page: &Page) -> String {
    // The illustrated home navigation keeps its layout; its document links share the same routes.
    if page.slug.is_empty() {
        return include_str!("../pages/home-nav.html")
            .replace("{{ guide-links }}", &links(page, DOCUMENT_LINKS));
    }
    // The demo keeps only the name at the top; its footer maps the site.
    if page.slug == "demo" {
        return format!(r#"<header class="site-header"><a class="site-brand" href="{}">board<br>cast</a></header>"#, root(page));
    }
    format!(
        r#"<header class="site-header wrap"><a class="site-brand" href="{}">board<br>cast</a><nav class="site-nav" aria-label="Main navigation">{}</nav></header>"#,
        root(page),
        links(page, &[0, 1, 2, 3, 4]),
    )
}

/// The demo ends on a plain map of the site: a heading over each group of links, as site footers go. The documents
/// have none: their navigation already links each other, and the home page ends on its last sheet.
fn footer(page: &Page) -> String {
    if page.slug != "demo" {
        return String::new();
    }
    let home = root(page);
    let releases = "https://github.com/HsiangNianian/boardcast/releases/latest";
    format!(
        r#"<footer class="sitemap">
  <a class="sitemap-brand" href="{home}">board<br>cast</a>
  <div><b>product</b><a href="{home}#story">talk &amp; draw</a><a href="{home}#story">circle</a><a href="{home}#story">replay</a></div>
  <div><b>get</b><a href="{home}#get">macos</a><a href="{home}#get">windows</a><a href="{home}#get">linux</a></div>
  <div><b>latest</b><a href="{releases}">{}</a><a href="https://github.com/HsiangNianian/boardcast/blob/main/CHANGELOG.md">changelog</a></div>
  <div><b>guide</b><a href="https://github.com/HsiangNianian/boardcast#get-started">start</a>{}</div>
</footer>"#,
        env!("CARGO_PKG_VERSION"),
        links(page, DOCUMENT_LINKS),
    )
}

/// A document's summary beside it, with the date it was last updated.
fn dated(page: &Page, content: String) -> String {
    if matches!(page.slug, "" | "demo") {
        return content;
    }
    content.replacen("</aside>", &format!(r#"<p class="updated label">updated {UPDATED}</p></aside>"#), 1)
}

fn render(page: &Page, bundled_head: &str, bundled_body: &str, assets: &Assets) -> String {
    let prefix = root(page);
    let document_style = if matches!(page.slug, "" | "demo") {
        ""
    } else {
        r#"<link rel="stylesheet" href="../pages.css">"#
    };
    let social = if page.slug.is_empty() {
        format!(
            r#"<meta property="og:title" content="{}"><meta property="og:description" content="A whiteboard that writes down your meeting, stroke by stroke, on your own computer.">"#,
            escape(page.title)
        )
    } else {
        String::new()
    };
    format!(
        r##"<!doctype html>
<!-- Generated by boardcast-site-gen; edit crates/site-gen/pages and its shared Rust layout. -->
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <meta name="description" content="{description}">
  {social}
  <meta name="theme-color" content="#000000">
  <link rel="icon" href="{prefix}favicon.png">
  {document_style}
  {bundled_head}
  <link rel="stylesheet" href="{prefix}chrome.css">
  <link rel="stylesheet" href="{prefix}appearance.css">
  <script src="{prefix}appearance.js"></script>
</head>
<body>
{content}
{footer}
{bundled_body}
</body>
</html>
"##,
        title = escape(page.title),
        footer = footer(page),
        description = escape(page.description),
        content = dated(page, fill_sizes(&page.content.replace("{{ navigation }}", &navigation(page)), assets)),
    )
}

fn contents<'a>(html: &'a str, tag: &str) -> io::Result<&'a str> {
    html.split_once(&format!("<{tag}>"))
        .and_then(|(_, rest)| rest.split_once(&format!("</{tag}>")))
        .map(|(content, _)| content)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Trunk output has no complete {tag} element"),
            )
        })
}

fn generate(staging: &Path, entry: &Path, assets: &Assets) -> io::Result<()> {
    let page = if entry.ends_with("demo/index.html") {
        &PAGES[1]
    } else {
        &PAGES[0]
    };
    let index = staging.join("index.html");
    let bundle = fs::read_to_string(&index)?;
    // Preserve hashed asset URLs, integrity attributes and Trunk's development reload script verbatim.
    let html = render(page, contents(&bundle, "head")?, contents(&bundle, "body")?, assets);
    fs::write(index, html)?;
    if page.slug.is_empty() {
        for page in &PAGES[2..] {
            let dir = staging.join(page.slug);
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("index.html"), render(page, "", "", assets))?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let staging = env::var("TRUNK_STAGING_DIR")?;
    let entry = env::var("TRUNK_HTML_FILE")?;
    let entry = Path::new(&entry);
    // Only the home page shows the downloads.
    let assets = if entry.ends_with("demo/index.html") { Assets::new() } else { release_assets()? };
    generate(Path::new(&staging), entry, &assets)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_have_complete_static_content_and_relative_navigation() {
        for page in &PAGES {
            let html = render(page, "", "", &Assets::new());
            assert!(html.contains(&format!("<title>{}</title>", escape(page.title))));
            assert!(html.contains("<h1>"));
            assert_eq!(html.contains(r#"<footer class="sitemap">"#), page.slug == "demo");
            assert!(!html.contains("{{ ") && !html.contains("last updated"));
            assert_eq!(html.matches("<html lang=\"en\">").count(), 1);
            assert!(html.contains(&format!("href=\"{}privacy/\"", root(page))));
            if !matches!(page.slug, "" | "demo") {
                assert!(html.contains(&format!(
                    "href=\"../{}/\" class=\"on\" aria-current=\"page\"",
                    page.slug
                )));
            }
            if !matches!(page.slug, "" | "demo") {
                assert!(!html.contains("type=\"module\""));
                assert!(html.contains(&format!(r#"<p class="updated label">updated {UPDATED}</p></aside>"#)));
            }
        }
        assert!(render(&PAGES[2], "", "", &Assets::new()).contains("<details open>"));
    }

    #[test]
    fn trunk_assets_and_reload_script_survive_rendering() {
        let bundle = r#"<!doctype html><html><head><link rel="stylesheet" href="./style-hash.css" integrity="sha384-test"><script type="module">import './demo-hash.js';</script></head><body><script>window.reloadTest = true;</script></body></html>"#;
        for page in &PAGES[..2] {
            let html = render(
                page,
                contents(bundle, "head").unwrap(),
                contents(bundle, "body").unwrap(),
                &Assets::new(),
            );
            assert!(html.contains(contents(bundle, "head").unwrap()));
            assert!(html.contains(contents(bundle, "body").unwrap()));
            assert_eq!(html.matches("<head>").count(), 1);
            assert_eq!(html.matches("<body>").count(), 1);
        }
        assert!(contents("<head>unfinished", "head").is_err());
    }

    #[test]
    fn download_sizes_come_from_the_release_packages() {
        let release = serde_json::json!({ "assets": [
            { "name": "boardcast-0.27.0-arm64-macos.zip", "size": 845_000_000u64 },
            { "name": "boardcast-0.27.0-x86_64-macos.zip", "size": 847_000_000u64 },
            { "name": "boardcast-0.27.0-x86_64-macos-none.zip", "size": 70_000_000u64 },
            { "name": "boardcast-0.27.0-x86_64-linux.tar.gz", "size": 876_246_171u64 },
            { "name": "boardcast-0.27.0-x86_64-linux-en.tar.gz", "size": 1_200_000_000u64 },
            { "name": "manifest.json", "size": 900u64 },
        ]});
        let assets = parse_assets(&release);
        assert_eq!(package_size(&assets, "macos"), Some(847_000_000));
        assert_eq!(package_size(&assets, "macos-none"), Some(70_000_000));
        assert_eq!(package_size(&assets, "linux"), Some(876_246_171));
        assert_eq!(package_size(&assets, "windows"), None);
        assert_eq!(human_size(876_246_171), "836 mb");
        assert_eq!(human_size(1_200_000_000), "1.1 gb");
        assert_eq!(
            fill_sizes("<em>{{ size:linux }}</em><em>{{ size:windows-ja }}</em>", &assets),
            "<em>836 mb</em><em></em>"
        );
        let home = render(&PAGES[0], "", "", &assets);
        assert!(home.contains("<em>808 mb</em>") && !home.contains("{{ size:"));
    }

    #[test]
    fn metadata_is_escaped_for_html_attributes() {
        assert_eq!(
            escape("A & B <\"board\">"),
            "A &amp; B &lt;&quot;board&quot;&gt;"
        );
    }
}
