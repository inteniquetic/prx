// Shared config generators for the prx benchmarks.
//
// Included with `include!` (rather than being its own crate) so both benchmark
// binaries build the same synthetic configs. Inner doc comments are not allowed
// in an included file, so the comments here are plain `//`.

/// Build a TOML config with `n` services and `n` routes plus one default route.
///
/// Route `i` is reachable at host `svc<i>.bench.local` with path prefix `/api/v<i>/`,
/// which lets benchmarks measure best case (first route), average (middle) and
/// worst case (last route) for a linear matcher.
#[allow(dead_code)]
pub fn config_toml(n: usize) -> String {
    let mut out = String::with_capacity(n * 320);
    out.push_str(
        "[server]\nlisten = [\"127.0.0.1:8080\"]\nhealth_path = \"/healthz\"\nready_path = \"/readyz\"\n\n",
    );

    for i in 0..n {
        out.push_str(&format!(
            "[[service]]\nname = \"svc{i}\"\nlb = \"round_robin\"\n\n\
             [[service.upstream]]\naddr = \"127.0.0.1:{}\"\nweight = 1\n\n",
            3000 + (i % 1000)
        ));
    }

    for i in 0..n {
        out.push_str(&format!(
            "[[route]]\nname = \"route{i}\"\nservice = \"svc{i}\"\n\
             host = \"svc{i}.bench.local\"\npath_prefix = \"/api/v{i}/\"\n\n"
        ));
    }

    out.push_str(
        "[[route]]\nname = \"default\"\nservice = \"svc0\"\nhost = \"\"\npath_prefix = \"/\"\nis_default = true\n",
    );
    out
}

/// Same as [`config_toml`] but every route matches a wildcard host, which is the
/// path that allocates a `String` per comparison in the current matcher.
#[allow(dead_code)]
pub fn wildcard_config_toml(n: usize) -> String {
    let mut out = String::with_capacity(n * 320);
    out.push_str("[server]\nlisten = [\"127.0.0.1:8080\"]\n\n");

    for i in 0..n {
        out.push_str(&format!(
            "[[service]]\nname = \"svc{i}\"\n\n[[service.upstream]]\naddr = \"127.0.0.1:{}\"\n\n",
            3000 + (i % 1000)
        ));
    }
    for i in 0..n {
        out.push_str(&format!(
            "[[route]]\nname = \"route{i}\"\nservice = \"svc{i}\"\n\
             host = \"*.tenant{i}.bench.local\"\npath_prefix = \"/api/\"\n\n"
        ));
    }
    out.push_str(
        "[[route]]\nname = \"default\"\nservice = \"svc0\"\npath_prefix = \"/\"\nis_default = true\n",
    );
    out
}
