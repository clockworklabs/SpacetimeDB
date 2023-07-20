pub fn cli() -> clap::Command {
    clap::Command::new("version")
        .about("Prints the version of SpacetimeDB")
        .after_help("Run `spacetimedb help version` for more detailed information.")
}

pub async fn exec() -> anyhow::Result<()> {
    // e.g. kubeadm version: &version.Info{Major:"1", Minor:"24", GitVersion:"v1.24.2", GitCommit:"f66044f4361b9f1f96f0053dd46cb7dce5e990a8", GitTreeState:"clean", BuildDate:"2022-06-15T14:20:54Z", GoVersion:"go1.18.3", Compiler:"gc", Platform:"linux/arm64"}
    println!("0.0.0");
    Ok(())
}
