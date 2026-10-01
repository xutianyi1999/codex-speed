fn main() {
    println!("cargo:rerun-if-changed=web/dist");
    assert!(
        std::path::Path::new("web/dist/index.html").exists(),
        "Build the embedded frontend first: cd web && pnpm install --frozen-lockfile && pnpm build"
    );
}
