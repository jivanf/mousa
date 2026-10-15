//! Restore shortened music names from publisher metadata; --apply updates only cached titles.
fn main() -> anyhow::Result<()> {
    mousa::titles::repair_cache(std::env::args().any(|arg| arg == "--apply"))
}
