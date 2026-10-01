fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_EMBEDDED_WEB");
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_WEB").is_none() {
        return;
    }
    println!("cargo:rerun-if-changed=web/dist");
    assert!(
        std::path::Path::new("web/dist/index.html").exists(),
        "Build the embedded frontend first: pnpm install --frozen-lockfile && pnpm build:frontend"
    );
}
