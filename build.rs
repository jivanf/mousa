fn main() {
    println!("cargo:rerun-if-changed=assets/mousa.ico");
    println!("cargo:rerun-if-changed=assets/windows.rc");
    #[cfg(windows)]
    embed_resource::compile("assets/windows.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile Windows application icon");
}
