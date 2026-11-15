//! What a TypeScript specifier needs: a Node builtin, or the package an import names.
//!
//! A `node:` specifier is a builtin of Node, whatever its name (a new builtin such as `node:sqlite` is written with
//! the prefix alone). A few names are builtins without it (`fs`, `path`): they are the ones Node had before it asked
//! for the prefix, and a name that is also a package on npm (`buffer`, `events`, `process`, `punycode`, `url`) is a
//! package when the manifest lists it ([`Specifier::Bare`]). A specifier that names a package is its name and not its
//! subpath: `lodash/fp` is `lodash`, and `@shop/kernel/utils` is `@shop/kernel`.

/// What a specifier of an import needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Specifier {
    /// A builtin of Node, or something that is no package: a URL scheme, a path, an alias such as `@/x`, a subpath
    /// import (`#name`)
    Neither,
    /// A name Node reads as a builtin without `node:`, which is a package when the manifest lists one of that name
    Bare(String),
    /// A package, by its name
    Package(String),
}

/// The builtins of Node that need no `node:` prefix: `module.builtinModules` of Node 22 and 24, the first part of each
/// (`fs/promises` is `fs`).
const BUILTINS: [&str; 56] = [
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_stream_duplex",
    "_stream_passthrough",
    "_stream_readable",
    "_stream_transform",
    "_stream_wrap",
    "_stream_writable",
    "_tls_common",
    "_tls_wrap",
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// What `specifier`, as an import writes it, needs. A relative path, a path from the root and a query are no package.
pub fn specifier(specifier: &str) -> Specifier {
    let specifier = specifier.split('?').next().unwrap_or(specifier);
    if specifier.is_empty()
        || specifier.starts_with(['.', '/', '#', '~'])
        || specifier.starts_with("@/")
    {
        return Specifier::Neither;
    }
    // A scheme (`node:`, `npm:`, `bun:`, `jsr:`, `data:`, `https:`, `virtual:`) names no package of the manifest
    if let Some((scheme, _)) = specifier.split_once(':')
        && !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphabetic())
    {
        return Specifier::Neither;
    }
    let name = package_name(specifier);
    if BUILTINS.contains(&name) {
        return Specifier::Bare(name.to_string());
    }
    Specifier::Package(name.to_string())
}

/// The package a specifier names: `lodash/fp` is `lodash`, `@shop/kernel/utils` is `@shop/kernel`.
pub fn package_name(specifier: &str) -> &str {
    let mut parts = specifier.splitn(3, '/');
    let first = parts.next().unwrap_or(specifier);
    if first.starts_with('@') {
        match parts.next() {
            Some(second) => &specifier[..first.len() + 1 + second.len()],
            None => first,
        }
    } else {
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str) -> Specifier {
        Specifier::Package(name.to_string())
    }

    #[test]
    fn a_specifier_names_the_package_and_not_its_subpath() {
        assert_eq!(specifier("react"), package("react"));
        assert_eq!(specifier("lodash/fp"), package("lodash"));
        assert_eq!(specifier("@shop/kernel"), package("@shop/kernel"));
        assert_eq!(specifier("@shop/kernel/utils/x"), package("@shop/kernel"));
        assert_eq!(specifier("vite/client?raw"), package("vite"));
    }

    #[test]
    fn a_builtin_with_the_prefix_a_path_an_alias_and_a_scheme_are_no_package() {
        for text in [
            "node:fs",
            "node:sqlite",
            "node:test",
            "./a",
            "../a",
            "/src/a",
            "#internal",
            "@/ui/atoms",
            "~/x",
            "npm:left-pad",
            "bun:sqlite",
            "virtual:uno.css",
            "https://esm.sh/x",
            "",
        ] {
            assert_eq!(specifier(text), Specifier::Neither, "{text}");
        }
    }

    #[test]
    fn a_name_node_has_without_the_prefix_is_bare_until_the_manifest_lists_one() {
        for (text, name) in [
            ("fs", "fs"),
            ("fs/promises", "fs"),
            ("buffer", "buffer"),
            ("path/posix", "path"),
        ] {
            assert_eq!(specifier(text), Specifier::Bare(name.to_string()), "{text}");
        }
        assert_eq!(specifier("fs-extra"), package("fs-extra"));
    }
}
